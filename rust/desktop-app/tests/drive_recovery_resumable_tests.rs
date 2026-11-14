mod common;

use common::{read_http_request, synthetic_credentials};
use emusync_data_drive::{
    DriveListing, DriveTransfer, ResumableUpload, TransferError, UploadError,
};
use emusync_data_local::{
    ConfigManager, RecoveryCredentials, SyncHistoryStore, TrackedError, TrackedFileSync,
    UploadContext, UploadSnapshot,
};
use emusync_desktop_app::{
    AuthorizedTransferFailure, OperationFailure, OperationSupervisor, RecoveryError,
    RecoveryStatus, StartError, reconcile_tracked_upload, reconcile_tracked_upload_with_listing,
    start_authorized_transfer,
};
use emusync_domain::{
    config::{AppConfig, ConfigurationStore, CredentialStore, Credentials},
    sync::RemoteState,
};
use reqwest::{Client, Url};
use std::{
    io::Write,
    net::TcpListener,
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn supervisor_retains_exclusion_and_snapshot_until_remote_reconciliation() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_http_request(&mut s).unwrap();
        write!(s, "HTTP/1.1 200 OK\r\nLocation: /session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        let (mut s, _) = listener.accept().unwrap();
        assert_eq!(read_http_request(&mut s).unwrap().1, b"abc");
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(s);
        for attempt in 0..7 {
            let (mut s, _) = listener.accept().unwrap();
            let (headers, _) = read_http_request(&mut s).unwrap();
            assert!(
                headers
                    .to_ascii_lowercase()
                    .contains("content-range: bytes */3")
            );
            if attempt == 0 {
                write!(s, "HTTP/1.1 308 Resume Incomplete\r\nRange: bytes=0-1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                continue;
            }
            if attempt == 1 {
                write!(
                    s,
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
                continue;
            }
            let body = if attempt == 2 {
                r#"{"id":"created-id","name":"save.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","version":"7"}"#
            } else if attempt == 3 {
                r#"{"id":"created-id","name":"save.dat","parents":["other"],"modifiedTime":"2026-01-01T00:00:00Z","sha256Checksum":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad","version":"7"}"#
            } else {
                r#"{"id":"created-id","name":"save.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","sha256Checksum":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad","version":"7"}"#
            };
            write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let credentials = synthetic_credentials(dir.path());
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let (t_hist, t_base, path) = (
        history.clone(),
        base.clone(),
        save.to_str().unwrap().to_owned(),
    );
    let mut supervisor = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let receiver = start_authorized_transfer(
        &mut supervisor,
        credentials.clone(),
        move |cancel, guard| async move {
            let transfer = DriveTransfer::new(
                Client::new(),
                t_base.clone(),
                t_base.clone(),
                t_base.clone(),
                t_base,
                cancel,
                Arc::new(|_| {}),
            );
            TrackedFileSync::new(transfer, t_hist)
                .upload_guarded("synthetic", &path, None, "folder", "save.dat", guard)
                .await
        },
    )
    .unwrap();
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    supervisor.cancel();
    assert!(receiver.await.unwrap().is_none());
    supervisor.shutdown().await;
    let failure = supervisor.failure().unwrap();
    let snapshot_path = {
        let guard = failure.lock().unwrap();
        match &*guard {
            OperationFailure::Operation(AuthorizedTransferFailure {
                error:
                    TrackedError::Upload {
                        source: TransferError::Upload(UploadError::OutcomeUnknown(_)),
                        snapshot,
                        context,
                    },
                ..
            }) => {
                assert_eq!(context.local, save);
                assert_eq!(context.folder, "folder");
                assert_eq!(context.remote_name, "save.dat");
                snapshot.path().to_owned()
            }
            other => panic!("unexpected {other:?}"),
        }
    };
    assert_eq!(std::fs::read(&snapshot_path).unwrap(), b"abc");
    assert!(matches!(
        supervisor.start(|_| async {
            Ok::<_, AuthorizedTransferFailure<emusync_data_local::ConfigManager>>(())
        }),
        Err(StartError::Busy)
    ));
    release_tx.send(()).unwrap();
    let resumable = ResumableUpload::new(Client::new(), base.clone());
    let transfer = DriveTransfer::new(
        Client::new(),
        base.clone(),
        base.clone(),
        base.clone(),
        base,
        tokio_util::sync::CancellationToken::new(),
        Arc::new(|_| {}),
    );
    let tracked = TrackedFileSync::new(transfer, history.clone());
    let key = history.key(&save, "folder", "save.dat").unwrap();
    assert_eq!(
        reconcile_tracked_upload(&mut supervisor, &resumable, &tracked, "synthetic")
            .await
            .unwrap(),
        RecoveryStatus::Incomplete { received: 2 }
    );
    assert_eq!(
        reconcile_tracked_upload(&mut supervisor, &resumable, &tracked, "synthetic")
            .await
            .unwrap(),
        RecoveryStatus::ExpiredOutcomeUnknown
    );
    assert_eq!(
        reconcile_tracked_upload(&mut supervisor, &resumable, &tracked, "synthetic")
            .await
            .unwrap(),
        RecoveryStatus::RemoteUnconfirmed
    );
    assert_eq!(
        reconcile_tracked_upload(&mut supervisor, &resumable, &tracked, "synthetic")
            .await
            .unwrap(),
        RecoveryStatus::RemoteUnconfirmed
    );
    assert!(history.get(&key).unwrap().is_none());
    assert!(matches!(
        supervisor.start(|_| async {
            Ok::<_, AuthorizedTransferFailure<emusync_data_local::ConfigManager>>(())
        }),
        Err(StartError::Busy)
    ));
    std::fs::write(&snapshot_path, b"tampered").unwrap();
    assert!(matches!(
        reconcile_tracked_upload(&mut supervisor, &resumable, &tracked, "synthetic").await,
        Err(RecoveryError::Local(_))
    ));
    assert!(history.get(&key).unwrap().is_none());
    std::fs::write(&snapshot_path, b"abc").unwrap();
    std::fs::write(&save, b"newer").unwrap();
    assert!(matches!(
        reconcile_tracked_upload(&mut supervisor, &resumable, &tracked, "synthetic").await,
        Err(RecoveryError::Local(_))
    ));
    assert_eq!(std::fs::read(&save).unwrap(), b"newer");
    assert_eq!(history.get(&key).unwrap().unwrap().remote_id, "created-id");
    assert!(matches!(
        supervisor.start(|_| async {
            Ok::<_, AuthorizedTransferFailure<emusync_data_local::ConfigManager>>(())
        }),
        Err(StartError::Busy)
    ));
    std::fs::write(&save, b"abc").unwrap();
    drop(failure);
    assert_eq!(
        reconcile_tracked_upload(&mut supervisor, &resumable, &tracked, "synthetic")
            .await
            .unwrap(),
        RecoveryStatus::Recovered
    );
    let checkpoint = history.get(&key).unwrap().unwrap();
    assert_eq!(checkpoint.remote_id, "created-id");
    assert_eq!(checkpoint.remote_version.as_deref(), Some("7"));
    assert!(!snapshot_path.exists());
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn completed_session_with_invalid_timestamp_requires_confirmed_metadata() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_http_request(&mut s).unwrap();
        write!(s, "HTTP/1.1 200 OK\r\nLocation: /session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        let (mut s, _) = listener.accept().unwrap();
        assert_eq!(read_http_request(&mut s).unwrap().1, b"abc");
        drop(s);
        let (mut s, _) = listener.accept().unwrap();
        assert!(
            read_http_request(&mut s)
                .unwrap()
                .0
                .to_ascii_lowercase()
                .contains("content-range: bytes */3")
        );
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        drop(s);
        for hash in [
            "wrong",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ] {
            let (mut s, _) = listener.accept().unwrap();
            assert!(
                read_http_request(&mut s)
                    .unwrap()
                    .0
                    .to_ascii_lowercase()
                    .contains("content-range: bytes */3")
            );
            let b1 = r#"{"id":"created","name":"save.dat"}"#;
            write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b1}",
                b1.len()
            )
            .unwrap();
            let (mut s, _) = listener.accept().unwrap();
            assert!(
                read_http_request(&mut s)
                    .unwrap()
                    .0
                    .starts_with("GET /files/created?")
            );
            let b2 = format!(
                r#"{{"id":"created","name":"save.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","sha256Checksum":"{hash}","version":"2"}}"#
            );
            write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b2}",
                b2.len()
            )
            .unwrap();
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let credentials = synthetic_credentials(dir.path());
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let (t_hist, t_base, path) = (
        history.clone(),
        base.clone(),
        save.to_str().unwrap().to_owned(),
    );
    let mut supervisor = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let receiver = start_authorized_transfer(
        &mut supervisor,
        credentials,
        move |cancel, guard| async move {
            let transfer = DriveTransfer::new(
                Client::new(),
                t_base.clone(),
                t_base.clone(),
                t_base.clone(),
                t_base,
                cancel,
                Arc::new(|_| {}),
            );
            TrackedFileSync::new(transfer, t_hist)
                .upload_guarded("synthetic", &path, None, "folder", "save.dat", guard)
                .await
        },
    )
    .unwrap();
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    supervisor.cancel();
    assert!(receiver.await.unwrap().is_none());
    release_tx.send(()).unwrap();
    let resumable = ResumableUpload::new(Client::new(), base.clone());
    let listing = DriveListing::new(Client::new(), base);
    let tracked = TrackedFileSync::new((), history.clone());
    assert_eq!(
        reconcile_tracked_upload_with_listing(
            &mut supervisor,
            &resumable,
            &listing,
            &tracked,
            "synthetic"
        )
        .await
        .unwrap(),
        RecoveryStatus::RemoteUnconfirmed
    );
    assert!(matches!(
        supervisor.start(|_| async { Ok::<_, AuthorizedTransferFailure<ConfigManager>>(()) }),
        Err(StartError::Busy)
    ));
    assert!(
        history
            .get(&history.key(&save, "folder", "save.dat").unwrap())
            .unwrap()
            .is_none()
    );
    assert_eq!(
        reconcile_tracked_upload_with_listing(
            &mut supervisor,
            &resumable,
            &listing,
            &tracked,
            "synthetic"
        )
        .await
        .unwrap(),
        RecoveryStatus::Recovered
    );
    assert_eq!(
        history
            .get(&history.key(&save, "folder", "save.dat").unwrap())
            .unwrap()
            .unwrap()
            .remote_id,
        "created"
    );
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn premature_creation_is_retained_and_recovered_only_with_matching_checksum() {
    let dir = tempfile::tempdir().unwrap();
    let credentials = synthetic_credentials(dir.path());
    let save = dir.path().join("save.dat");
    std::fs::write(&save, vec![7; 8 * 1024 * 1024 + 1024]).unwrap();
    let digest = emusync_data_local::file_sha256(&save).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_http_request(&mut s).unwrap();
        write!(s, "HTTP/1.1 200 OK\r\nLocation: /session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        let (mut s, _) = listener.accept().unwrap();
        assert_eq!(read_http_request(&mut s).unwrap().1.len(), 8 * 1024 * 1024);
        let b = r#"{"id":"created","name":"save.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z"}"#;
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
            b.len()
        )
        .unwrap();
        for (parent, hash) in [
            ("other", digest.as_str()),
            ("folder", "wrong"),
            ("folder", digest.as_str()),
        ] {
            let (mut s, _) = listener.accept().unwrap();
            assert!(
                read_http_request(&mut s)
                    .unwrap()
                    .0
                    .starts_with("GET /files/created?")
            );
            let body = format!(
                r#"{{"id":"created","name":"save.dat","parents":["{parent}"],"modifiedTime":"2026-01-01T00:00:00Z","sha256Checksum":"{hash}","version":"2"}}"#
            );
            write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let (t_hist, t_base, path) = (
        history.clone(),
        base.clone(),
        save.to_str().unwrap().to_owned(),
    );
    let mut supervisor = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let result = start_authorized_transfer(
        &mut supervisor,
        credentials,
        move |cancel, guard| async move {
            let transfer = DriveTransfer::new(
                Client::new(),
                t_base.clone(),
                t_base.clone(),
                t_base.clone(),
                t_base,
                cancel,
                Arc::new(|_| {}),
            );
            TrackedFileSync::new(transfer, t_hist)
                .upload_guarded("synthetic", &path, None, "folder", "save.dat", guard)
                .await
        },
    )
    .unwrap();
    assert!(result.await.unwrap().is_none());
    supervisor.shutdown().await;
    let resumable = ResumableUpload::new(Client::new(), base.clone());
    let listing = DriveListing::new(Client::new(), base);
    let tracked = TrackedFileSync::new((), history.clone());
    for _ in 0..2 {
        assert_eq!(
            reconcile_tracked_upload_with_listing(
                &mut supervisor,
                &resumable,
                &listing,
                &tracked,
                "synthetic"
            )
            .await
            .unwrap(),
            RecoveryStatus::RemoteUnconfirmed
        );
        assert!(matches!(
            supervisor.start(|_| async { Ok::<_, AuthorizedTransferFailure<ConfigManager>>(()) }),
            Err(StartError::Busy)
        ));
    }
    assert_eq!(
        reconcile_tracked_upload_with_listing(
            &mut supervisor,
            &resumable,
            &listing,
            &tracked,
            "synthetic"
        )
        .await
        .unwrap(),
        RecoveryStatus::Recovered
    );
    assert_eq!(
        history
            .get(&history.key(&save, "folder", "save.dat").unwrap())
            .unwrap()
            .unwrap()
            .remote_id,
        "created"
    );
    server.join().unwrap();
}

#[tokio::test]
async fn recovery_rejects_reconnected_credentials_and_retains_snapshot_and_exclusion() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = Arc::new(ConfigManager::new(&config_path).unwrap());
    let config = AppConfig {
        language: Default::default(),
        credentials: Some(Credentials {
            client_id: "client".into(),
            client_secret: "synthetic".into(),
            refresh_token: Some("refresh".into()),
        }),
        entries: vec![],
    };
    manager.save(&config).unwrap();
    let proof = manager.snapshot(&config).unwrap();
    let credentials = RecoveryCredentials::new(manager.clone(), proof, config.clone());
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let snapshot = UploadSnapshot::prepare(&save).unwrap();
    let snapshot_path = snapshot.path().to_owned();
    let remote = RemoteState {
        id: "remote".into(),
        modified: 1_700_000_000_000_000_000,
        sha256: Some(snapshot.digest().into()),
        version: Some("2".into()),
    };
    let context = UploadContext {
        local: save.clone(),
        folder: "folder".into(),
        remote_name: "save.dat".into(),
        observed: None,
        recovery_id: None,
    };
    let mut supervisor = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let receiver =
        start_authorized_transfer(&mut supervisor, credentials, move |_, _| async move {
            Err::<(), _>(TrackedError::AfterUpload {
                source: std::io::Error::other("retry"),
                snapshot,
                remote: Box::new(remote),
                context: Box::new(context),
            })
        })
        .unwrap();
    assert!(receiver.await.unwrap().is_none());
    supervisor.shutdown().await;
    manager.update_credentials(|_| None).unwrap();
    manager
        .update_credentials(|_| config.credentials.clone())
        .unwrap();
    let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
    let tracked = TrackedFileSync::new((), history.clone());
    let resumable = ResumableUpload::new(
        Client::new(),
        Url::parse("http://127.0.0.1:1/upload").unwrap(),
    );
    let result = reconcile_tracked_upload(&mut supervisor, &resumable, &tracked, "synthetic").await;
    assert!(
        matches!(result, Err(RecoveryError::Local(e)) if e.kind() == std::io::ErrorKind::PermissionDenied)
    );
    assert!(matches!(
        supervisor.start(|_| async { Ok::<_, AuthorizedTransferFailure<ConfigManager>>(()) }),
        Err(StartError::Busy)
    ));
    assert_eq!(std::fs::read(&snapshot_path).unwrap(), b"abc");
    assert!(
        history
            .get(&history.key(&save, "folder", "save.dat").unwrap())
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn reconcile_tracked_upload_rejects_unsupported_failure_variants_without_releasing_permit() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = Arc::new(ConfigManager::new(&config_path).unwrap());
    let config = AppConfig {
        language: Default::default(),
        credentials: Some(Credentials {
            client_id: "client".into(),
            client_secret: "synthetic".into(),
            refresh_token: Some("refresh".into()),
        }),
        entries: vec![],
    };
    manager.save(&config).unwrap();
    let snapshot = manager.snapshot(&config).unwrap();
    let credentials = RecoveryCredentials::new(manager.clone(), snapshot, config);
    let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
    let tracked = TrackedFileSync::new((), history);
    let resumable = ResumableUpload::new(
        Client::new(),
        Url::parse("http://127.0.0.1:1/upload").unwrap(),
    );

    // Scenario 1: OperationFailure::Panic
    let mut supervisor1 = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    assert!(supervisor1.retain_failure(OperationFailure::Panic));
    let err1 = reconcile_tracked_upload(&mut supervisor1, &resumable, &tracked, "synthetic").await;
    assert!(matches!(err1, Err(RecoveryError::UnsupportedFailure)));
    assert!(supervisor1.failure().is_some());
    assert!(matches!(
        supervisor1.start(|_| async { Ok::<_, AuthorizedTransferFailure<ConfigManager>>(()) }),
        Err(StartError::Busy)
    ));

    // Scenario 2: TrackedError::Local
    let mut supervisor2 = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let local_err = TrackedError::Local(std::io::Error::other("disk error"));
    let failure2 = AuthorizedTransferFailure::new(local_err, credentials);
    assert!(supervisor2.retain_failure(OperationFailure::Operation(failure2)));
    let err2 = reconcile_tracked_upload(&mut supervisor2, &resumable, &tracked, "synthetic").await;
    assert!(matches!(err2, Err(RecoveryError::UnsupportedFailure)));
    assert!(supervisor2.failure().is_some());
    assert!(matches!(
        supervisor2.start(|_| async { Ok::<_, AuthorizedTransferFailure<ConfigManager>>(()) }),
        Err(StartError::Busy)
    ));
}

#[tokio::test]
async fn reconcile_tracked_upload_returns_failure_still_referenced_when_external_arc_is_held() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = Arc::new(ConfigManager::new(&config_path).unwrap());
    let config = AppConfig {
        language: Default::default(),
        credentials: Some(Credentials {
            client_id: "client".into(),
            client_secret: "synthetic".into(),
            refresh_token: Some("refresh".into()),
        }),
        entries: vec![],
    };
    manager.save(&config).unwrap();
    let proof = manager.snapshot(&config).unwrap();
    let credentials = RecoveryCredentials::new(manager.clone(), proof, config);
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let snapshot = UploadSnapshot::prepare(&save).unwrap();
    let remote = RemoteState {
        id: "created-id".into(),
        modified: 1_700_000_000_000_000_000,
        sha256: Some(snapshot.digest().into()),
        version: Some("1".into()),
    };
    let context = UploadContext {
        local: save.clone(),
        folder: "folder".into(),
        remote_name: "save.dat".into(),
        observed: None,
        recovery_id: None,
    };
    let mut supervisor = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let receiver =
        start_authorized_transfer(&mut supervisor, credentials, move |_, _| async move {
            Err::<(), _>(TrackedError::AfterUpload {
                source: std::io::Error::other("after-upload-fail"),
                snapshot,
                remote: Box::new(remote),
                context: Box::new(context),
            })
        })
        .unwrap();
    assert!(receiver.await.unwrap().is_none());
    supervisor.shutdown().await;

    // External Arc held
    let leaked_ref = supervisor.failure().unwrap();

    let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
    let tracked = TrackedFileSync::new((), history.clone());
    let resumable = ResumableUpload::new(
        Client::new(),
        Url::parse("http://127.0.0.1:1/upload").unwrap(),
    );

    let res = reconcile_tracked_upload(&mut supervisor, &resumable, &tracked, "synthetic").await;
    assert!(matches!(res, Err(RecoveryError::FailureStillReferenced)));

    let key = history.key(&save, "folder", "save.dat").unwrap();
    assert_eq!(history.get(&key).unwrap().unwrap().remote_id, "created-id");
    assert!(matches!(
        supervisor.start(|_| async { Ok::<_, AuthorizedTransferFailure<ConfigManager>>(()) }),
        Err(StartError::Busy)
    ));

    drop(leaked_ref);
    assert!(supervisor.resolve());
    assert!(supervisor.failure().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconcile_tracked_upload_with_listing_rejects_folder_or_mismatched_name_from_listing() {
    let dir = tempfile::tempdir().unwrap();
    let credentials = synthetic_credentials(dir.path());
    let save = dir.path().join("save.dat");
    std::fs::write(&save, vec![7; 8 * 1024 * 1024 + 1024]).unwrap();
    let digest = emusync_data_local::file_sha256(&save).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let digest_clone = digest.clone();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_http_request(&mut s).unwrap();
        write!(s, "HTTP/1.1 200 OK\r\nLocation: /session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        let (mut s, _) = listener.accept().unwrap();
        assert_eq!(read_http_request(&mut s).unwrap().1.len(), 8 * 1024 * 1024);
        let b = r#"{"id":"created","name":"save.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z"}"#;
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
            b.len()
        )
        .unwrap();

        // Attempt 1: is_folder true (folder MIME type) with matching SHA-256
        let (mut s, _) = listener.accept().unwrap();
        assert!(
            read_http_request(&mut s)
                .unwrap()
                .0
                .starts_with("GET /files/created?")
        );
        let b1 = format!(
            r#"{{"id":"created","name":"save.dat","mimeType":"application/vnd.google-apps.folder","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","sha256Checksum":"{digest_clone}","version":"2"}}"#
        );
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b1}",
            b1.len()
        )
        .unwrap();

        // Attempt 2: wrong name ("other.dat") with matching SHA-256
        let (mut s, _) = listener.accept().unwrap();
        assert!(
            read_http_request(&mut s)
                .unwrap()
                .0
                .starts_with("GET /files/created?")
        );
        let b2 = format!(
            r#"{{"id":"created","name":"other.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","sha256Checksum":"{digest_clone}","version":"2"}}"#
        );
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b2}",
            b2.len()
        )
        .unwrap();
    });

    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let (t_hist, t_base, path) = (
        history.clone(),
        base.clone(),
        save.to_str().unwrap().to_owned(),
    );
    let mut supervisor = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let result = start_authorized_transfer(
        &mut supervisor,
        credentials,
        move |cancel, guard| async move {
            let transfer = DriveTransfer::new(
                Client::new(),
                t_base.clone(),
                t_base.clone(),
                t_base.clone(),
                t_base,
                cancel,
                Arc::new(|_| {}),
            );
            TrackedFileSync::new(transfer, t_hist)
                .upload_guarded("synthetic", &path, None, "folder", "save.dat", guard)
                .await
        },
    )
    .unwrap();
    assert!(result.await.unwrap().is_none());
    supervisor.shutdown().await;

    let resumable = ResumableUpload::new(Client::new(), base.clone());
    let listing = DriveListing::new(Client::new(), base);
    let tracked = TrackedFileSync::new((), history.clone());

    for _ in 0..2 {
        let status = reconcile_tracked_upload_with_listing(
            &mut supervisor,
            &resumable,
            &listing,
            &tracked,
            "synthetic",
        )
        .await
        .unwrap();
        assert_eq!(status, RecoveryStatus::RemoteUnconfirmed);
        assert!(matches!(
            supervisor.start(|_| async { Ok::<_, AuthorizedTransferFailure<ConfigManager>>(()) }),
            Err(StartError::Busy)
        ));
    }

    let key = history.key(&save, "folder", "save.dat").unwrap();
    assert!(history.get(&key).unwrap().is_none());
    server.join().unwrap();
}
