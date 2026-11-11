mod common;

use common::{read_http_request, synthetic_credentials};
use emusync_data_drive::{DriveTransfer, ResumableUpload};
use emusync_data_local::{
    ConfigManager, RecoveryCredentials, SyncHistoryStore, TrackedError, TrackedFileSync,
};
use emusync_desktop_app::{
    AuthorizedTransferFailure, CloseState, OperationFailure, OperationSupervisor, RecoveryStatus,
    RuntimeOwner, reconcile_tracked_upload, start_authorized_transfer,
};
use emusync_domain::config::{AppConfig, ConfigurationStore, CredentialStore, Credentials};
#[cfg(target_os = "linux")]
use emusync_domain::sync::RemoteState;
use reqwest::{Client, Url};
use std::{
    io::Write,
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

#[tokio::test]
async fn stale_authorization_never_starts_transfer() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(ConfigManager::new(&dir.path().join("config.json")).unwrap());
    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "client".into(),
            client_secret: "synthetic".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![],
    };
    store.save(&config).unwrap();
    let guard = RecoveryCredentials::new(
        store.clone(),
        store.snapshot(&config).unwrap(),
        config.clone(),
    );
    store.update_credentials(|_| None).unwrap();
    store
        .update_credentials(|_| config.credentials.clone())
        .unwrap();
    let entered = Arc::new(AtomicBool::new(false));
    let entered_task = entered.clone();
    let mut supervisor = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let result = start_authorized_transfer(&mut supervisor, guard, move |_, _| async move {
        entered_task.store(true, Ordering::SeqCst);
        Ok::<_, emusync_data_local::TrackedError<emusync_data_drive::TransferError>>(())
    })
    .unwrap();
    assert!(result.await.unwrap().is_none());
    supervisor.shutdown().await;
    assert!(!entered.load(Ordering::SeqCst));
    let failure = supervisor.failure().unwrap();
    let guard = failure.lock().unwrap();
    assert!(
        matches!(&*guard, OperationFailure::Operation(failure) if matches!(&failure.error, TrackedError::Local(error) if error.kind() == std::io::ErrorKind::PermissionDenied))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connection_change_during_successful_upload_cannot_record_old_history() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = Arc::new(ConfigManager::new(&config_path).unwrap());
    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "client".into(),
            client_secret: "synthetic".into(),
            refresh_token: Some("refresh".into()),
        }),
        entries: vec![],
    };
    manager.save(&config).unwrap();
    let guard = RecoveryCredentials::new(
        manager.clone(),
        manager.snapshot(&config).unwrap(),
        config.clone(),
    );
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let changing = manager.clone();
    let original = config.credentials.clone();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_http_request(&mut s).unwrap();
        write!(
            s,
            "HTTP/1.1 200 OK\r\nLocation: /session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let (mut s, _) = listener.accept().unwrap();
        let (_, body) = read_http_request(&mut s).unwrap();
        assert_eq!(body, b"abc");
        changing.update_credentials(|_| None).unwrap();
        changing.update_credentials(|_| original).unwrap();
        let body = r#"{"id":"created","name":"save.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","sha256Checksum":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad","version":"2"}"#;
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let task_base = base;
    let task_path = save.to_str().unwrap().to_owned();
    let task_history = history.clone();
    let mut supervisor = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let receiver = start_authorized_transfer(
        &mut supervisor,
        guard,
        move |cancel, operation_guard| async move {
            let transfer = DriveTransfer::new(
                Client::new(),
                task_base.clone(),
                task_base.clone(),
                task_base.clone(),
                task_base,
                cancel,
                Arc::new(|_| {}),
            );
            TrackedFileSync::new(transfer, task_history)
                .upload_guarded(
                    "synthetic",
                    &task_path,
                    None,
                    "folder",
                    "save.dat",
                    operation_guard,
                )
                .await
        },
    )
    .unwrap();
    assert!(receiver.await.unwrap().is_none());
    supervisor.shutdown().await;
    let failure = supervisor.failure().unwrap();
    let locked = failure.lock().unwrap();
    assert!(
        matches!(&*locked, OperationFailure::Operation(failure) if matches!(&failure.error, TrackedError::AfterUpload { source, remote, .. } if source.kind() == std::io::ErrorKind::PermissionDenied && remote.id == "created"))
    );
    let key = history.key(&save, "folder", "save.dat").unwrap();
    assert!(history.get(&key).unwrap().is_none());
    assert_eq!(std::fs::read(&save).unwrap(), b"abc");
    server.join().unwrap();
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn changed_connection_during_pinned_download_preserves_save_and_history() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = Arc::new(ConfigManager::new(&config_path).unwrap());
    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "client".into(),
            client_secret: "synthetic".into(),
            refresh_token: Some("refresh".into()),
        }),
        entries: vec![],
    };
    manager.save(&config).unwrap();
    let guard = RecoveryCredentials::new(
        manager.clone(),
        manager.snapshot(&config).unwrap(),
        config.clone(),
    );
    let root = dir.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let save = root.join("save.dat");
    std::fs::write(&save, b"original").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let changing = manager.clone();
    let original = config.credentials.clone();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let (headers, _) = read_http_request(&mut s).unwrap();
        assert!(headers.starts_with("GET /files/remote?"));
        changing.update_credentials(|_| None).unwrap();
        changing.update_credentials(|_| original).unwrap();
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc"
        )
        .unwrap();
    });
    let remote = RemoteState {
        id: "remote".into(),
        modified: 1_700_000_000_000_000_000,
        sha256: Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into()),
        version: Some("1".into()),
    };
    let task_history = history.clone();
    let task_root = root.to_str().unwrap().to_owned();
    let mut supervisor = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let receiver = start_authorized_transfer(
        &mut supervisor,
        guard,
        move |cancel, credentials| async move {
            let transfer = DriveTransfer::new(
                Client::new(),
                base.clone(),
                base.clone(),
                base.clone(),
                base,
                cancel,
                Arc::new(|_| {}),
            );
            TrackedFileSync::new(transfer, task_history)
                .download_pinned_guarded(
                    "synthetic",
                    &remote,
                    &task_root,
                    "folder",
                    "save.dat",
                    credentials,
                )
                .await
        },
    )
    .unwrap();
    assert!(receiver.await.unwrap().is_none());
    supervisor.shutdown().await;
    let failure = supervisor.failure().unwrap();
    let locked = failure.lock().unwrap();
    assert!(
        matches!(&*locked, OperationFailure::Operation(failure) if matches!(&failure.error, TrackedError::Local(error) if error.kind() == std::io::ErrorKind::PermissionDenied))
    );
    assert_eq!(std::fs::read(&save).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    let key = history.key(&save, "folder", "save.dat").unwrap();
    assert!(history.get(&key).unwrap().is_none());
    assert!(!history.backup_directory().exists());
    server.join().unwrap();
}

#[test]
fn runtime_close_waits_for_reconciliation_and_history_before_exit() {
    let dir = tempfile::tempdir().unwrap();
    let guard = synthetic_credentials(dir.path());
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let (put_tx, put_rx) = mpsc::channel();
    let (put_release_tx, put_release_rx) = mpsc::channel();
    let (query_tx, query_rx) = mpsc::channel();
    let (query_release_tx, query_release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_http_request(&mut s).unwrap();
        write!(
            s,
            "HTTP/1.1 200 OK\r\nLocation: /session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let (mut s, _) = listener.accept().unwrap();
        let (_, body) = read_http_request(&mut s).unwrap();
        assert_eq!(body, b"abc");
        put_tx.send(()).unwrap();
        put_release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(s);
        let (mut s, _) = listener.accept().unwrap();
        let (headers, _) = read_http_request(&mut s).unwrap();
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes */3")
        );
        query_tx.send(()).unwrap();
        query_release_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        let body = r#"{"id":"created","name":"save.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","sha256Checksum":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad","version":"2"}"#;
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let mut owner = RuntimeOwner::<AuthorizedTransferFailure<ConfigManager>>::new().unwrap();
    let task_base = base.clone();
    let task_history = history.clone();
    let task_path = save.to_str().unwrap().to_owned();
    let receiver = owner
        .with_supervisor(|supervisor| {
            start_authorized_transfer(supervisor, guard, move |cancel, credentials| async move {
                let transfer = DriveTransfer::new(
                    Client::new(),
                    task_base.clone(),
                    task_base.clone(),
                    task_base.clone(),
                    task_base,
                    cancel,
                    Arc::new(|_| {}),
                );
                TrackedFileSync::new(transfer, task_history)
                    .upload_guarded(
                        "synthetic",
                        &task_path,
                        None,
                        "folder",
                        "save.dat",
                        credentials,
                    )
                    .await
            })
        })
        .unwrap()
        .unwrap();
    put_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    owner.begin_close();
    assert!(receiver.blocking_recv().unwrap().is_none());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match owner.poll_close() {
            CloseState::Pending => {
                assert!(Instant::now() < deadline);
                thread::yield_now();
            }
            CloseState::RecoveryRequired => break,
            CloseState::Ready => panic!("unresolved write released"),
        }
    }
    put_release_tx.send(()).unwrap();
    let recovery_history = history.clone();
    let recovery_base = base.clone();
    let recovery = owner
        .start_recovery(move |supervisor| {
            Box::pin(async move {
                let resumable = ResumableUpload::new(Client::new(), recovery_base);
                let tracked = TrackedFileSync::new((), recovery_history);
                reconcile_tracked_upload(supervisor, &resumable, &tracked, "synthetic").await
            })
        })
        .unwrap();
    query_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    owner.begin_close();
    assert_eq!(owner.poll_close(), CloseState::Pending);
    query_release_tx.send(()).unwrap();
    assert_eq!(
        recovery.blocking_recv().unwrap().unwrap().unwrap(),
        RecoveryStatus::Recovered
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match owner.poll_close() {
            CloseState::Pending => {
                assert!(Instant::now() < deadline);
                thread::yield_now();
            }
            CloseState::Ready => break,
            CloseState::RecoveryRequired => panic!("recovery failed"),
        }
    }
    let key = history.key(&save, "folder", "save.dat").unwrap();
    assert_eq!(history.get(&key).unwrap().unwrap().remote_id, "created");
    assert_eq!(std::fs::read(&save).unwrap(), b"abc");
    server.join().unwrap();
}
