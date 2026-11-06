use emusync_data_drive::DriveTransfer;
use emusync_data_local::{SyncHistoryStore, TrackedFileSync};
use reqwest::{Client, Url};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    thread,
};
fn synthetic_credentials(
    root: &std::path::Path,
) -> emusync_data_local::RecoveryCredentials<emusync_data_local::ConfigManager> {
    use emusync_domain::config::{AppConfig, CredentialStore, Credentials};
    let store =
        Arc::new(emusync_data_local::ConfigManager::new(&root.join("config.json")).unwrap());
    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "client".into(),
            client_secret: "synthetic".into(),
            refresh_token: Some("refresh".into()),
        }),
        entries: vec![],
    };
    store.save(&config).unwrap();
    let snapshot = store.snapshot(&config).unwrap();
    emusync_data_local::RecoveryCredentials::new(store, snapshot, config)
}
fn request(stream: &mut std::net::TcpStream) -> (String, Vec<u8>) {
    let mut data = Vec::new();
    let mut buf = [0; 4096];
    let end = loop {
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0);
        data.extend_from_slice(&buf[..n]);
        if let Some(i) = data.windows(4).position(|b| b == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let headers = String::from_utf8(data[..end].to_vec()).unwrap();
    let len = headers
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .and_then(|v| v.parse::<usize>().ok())
        })
        .unwrap_or(0);
    while data.len() < end + len {
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0);
        data.extend_from_slice(&buf[..n]);
    }
    (headers, data[end..end + len].to_vec())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn supervisor_retains_exclusion_and_snapshot_until_remote_reconciliation() {
    use emusync_data_drive::{ResumableUpload, TransferError, UploadError};
    use emusync_data_local::TrackedError;
    use emusync_desktop_app::{
        AuthorizedTransferFailure, OperationFailure, OperationSupervisor, RecoveryError,
        RecoveryStatus, StartError, reconcile_tracked_upload, start_authorized_transfer,
    };
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        request(&mut s);
        write!(s,"HTTP/1.1 200 OK\r\nLocation: /session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        let (mut s, _) = listener.accept().unwrap();
        let (_, body) = request(&mut s);
        assert_eq!(body, b"abc");
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(s);
        for attempt in 0..7 {
            let (mut s, _) = listener.accept().unwrap();
            let (headers, _) = request(&mut s);
            assert!(
                headers
                    .to_ascii_lowercase()
                    .contains("content-range: bytes */3")
            );
            if attempt == 0 {
                write!(s,"HTTP/1.1 308 Resume Incomplete\r\nRange: bytes=0-1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
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
            write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let credentials = synthetic_credentials(dir.path());
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let task_history = history.clone();
    let task_base = base.clone();
    let path = save.to_str().unwrap().to_owned();
    let mut supervisor =
        OperationSupervisor::<AuthorizedTransferFailure<emusync_data_local::ConfigManager>>::new();
    let receiver = start_authorized_transfer(
        &mut supervisor,
        credentials.clone(),
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
                    &path,
                    None,
                    "folder",
                    "save.dat",
                    operation_guard,
                )
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
    use emusync_data_drive::{DriveListing, ResumableUpload};
    use emusync_desktop_app::{
        AuthorizedTransferFailure, OperationSupervisor, RecoveryStatus, StartError,
        reconcile_tracked_upload_with_listing, start_authorized_transfer,
    };
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        request(&mut stream);
        write!(stream, "HTTP/1.1 200 OK\r\nLocation: /session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        assert_eq!(request(&mut stream).1, b"abc");
        drop(stream);
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, body) = request(&mut stream);
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes */3")
        );
        assert!(body.is_empty());
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        drop(stream);
        for hash in [
            "wrong",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            let (headers, body) = request(&mut stream);
            assert!(
                headers
                    .to_ascii_lowercase()
                    .contains("content-range: bytes */3")
            );
            assert!(body.is_empty());
            let body = r#"{"id":"created","name":"save.dat"}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            let (mut stream, _) = listener.accept().unwrap();
            let (headers, _) = request(&mut stream);
            assert!(headers.starts_with("GET /files/created?"));
            let body = format!(
                r#"{{"id":"created","name":"save.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","sha256Checksum":"{hash}","version":"2"}}"#
            );
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let credentials = synthetic_credentials(dir.path());
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let task_history = history.clone();
    let task_base = base.clone();
    let path = save.to_str().unwrap().to_owned();
    let mut supervisor =
        OperationSupervisor::<AuthorizedTransferFailure<emusync_data_local::ConfigManager>>::new();
    let receiver = start_authorized_transfer(
        &mut supervisor,
        credentials,
        move |cancel, guard| async move {
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
        supervisor.start(|_| async {
            Ok::<_, AuthorizedTransferFailure<emusync_data_local::ConfigManager>>(())
        }),
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
    use emusync_data_drive::{DriveListing, ResumableUpload};
    use emusync_desktop_app::{
        AuthorizedTransferFailure, OperationSupervisor, RecoveryStatus, StartError,
        reconcile_tracked_upload, reconcile_tracked_upload_with_listing, start_authorized_transfer,
    };
    let dir = tempfile::tempdir().unwrap();
    let credentials = synthetic_credentials(dir.path());
    let save = dir.path().join("save.dat");
    std::fs::write(&save, vec![7; 8 * 1024 * 1024 + 1024]).unwrap();
    let digest = emusync_data_local::file_sha256(&save).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        request(&mut stream);
        write!(stream, "HTTP/1.1 200 OK\r\nLocation: /session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        let (_, body) = request(&mut stream);
        assert_eq!(body.len(), 8 * 1024 * 1024);
        let body = r#"{"id":"created","name":"save.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        for (parent, hash) in [
            ("other", digest.as_str()),
            ("folder", "wrong"),
            ("folder", digest.as_str()),
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            let (headers, _) = request(&mut stream);
            assert!(headers.starts_with("GET /files/created?"));
            let body = format!(
                r#"{{"id":"created","name":"save.dat","parents":["{parent}"],"modifiedTime":"2026-01-01T00:00:00Z","sha256Checksum":"{hash}","version":"2"}}"#
            );
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let task_history = history.clone();
    let task_base = base.clone();
    let path = save.to_str().unwrap().to_owned();
    let mut supervisor =
        OperationSupervisor::<AuthorizedTransferFailure<emusync_data_local::ConfigManager>>::new();
    let result = start_authorized_transfer(
        &mut supervisor,
        credentials,
        move |cancel, guard| async move {
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
                .upload_guarded("synthetic", &path, None, "folder", "save.dat", guard)
                .await
        },
    )
    .unwrap();
    assert!(result.await.unwrap().is_none());
    let resumable = ResumableUpload::new(Client::new(), base.clone());
    let listing = DriveListing::new(Client::new(), base);
    let tracked = TrackedFileSync::new((), history.clone());
    assert_eq!(
        reconcile_tracked_upload(&mut supervisor, &resumable, &tracked, "synthetic")
            .await
            .unwrap(),
        RecoveryStatus::RemoteUnconfirmed
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
        RecoveryStatus::RemoteUnconfirmed
    );
    assert!(matches!(
        supervisor.start(|_| async {
            Ok::<_, AuthorizedTransferFailure<emusync_data_local::ConfigManager>>(())
        }),
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
        RecoveryStatus::RemoteUnconfirmed
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
async fn uncertain_conditional_write_reconciles_only_after_matching_hash() {
    use emusync_data_drive::DriveListing;
    use emusync_desktop_app::{
        AuthorizedTransferFailure, OperationSupervisor, RecoveryStatus, StartError,
        reconcile_conditional_upload, start_authorized_transfer,
    };
    use emusync_domain::sync::RemoteState;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let v3 = Url::parse(&format!("{root}v3")).unwrap();
    let v2 = Url::parse(&format!("{root}v2")).unwrap();
    let upload = Url::parse(&format!("{root}upload")).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let (headers, _) = request(&mut s);
        assert!(headers.starts_with("GET /v2/old?"));
        let body = r#"{"id":"old","title":"save.dat","modifiedDate":"2026-01-01T00:00:00Z","etag":"\"v1\"","version":"1"}"#;
        write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = request(&mut s);
        assert!(headers.starts_with("PUT /upload/old?"));
        assert!(headers.to_ascii_lowercase().contains("if-match: \"v1\""));
        assert_eq!(body, b"abc");
        drop(s);
        for n in 0..3 {
            let (mut s, _) = listener.accept().unwrap();
            let (headers, _) = request(&mut s);
            assert!(headers.starts_with("GET /v3/old?"));
            let hash = match n {
                0 => "",
                1 => ",\"sha256Checksum\":\"wrong\"",
                _ => {
                    ",\"sha256Checksum\":\"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\""
                }
            };
            let body = format!(
                r#"{{"id":"old","name":"save.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","version":"2"{hash}}}"#
            );
            write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let credentials = synthetic_credentials(dir.path());
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let task_history = history.clone();
    let observed = RemoteState {
        id: "old".into(),
        modified: 1_700_000_000_000_000_000,
        sha256: None,
        version: Some("1".into()),
    };
    let path = save.to_str().unwrap().to_owned();
    let task_v3 = v3.clone();
    let task_v2 = v2.clone();
    let task_upload = upload.clone();
    let mut supervisor =
        OperationSupervisor::<AuthorizedTransferFailure<emusync_data_local::ConfigManager>>::new();
    let result = start_authorized_transfer(
        &mut supervisor,
        credentials.clone(),
        move |cancel, operation_guard| async move {
            let transfer = DriveTransfer::new(
                Client::new(),
                task_v3.clone(),
                task_v3,
                task_v2,
                task_upload,
                cancel,
                Arc::new(|_| {}),
            );
            TrackedFileSync::new(transfer, task_history)
                .upload_guarded(
                    "synthetic",
                    &path,
                    Some(&observed),
                    "folder",
                    "save.dat",
                    operation_guard,
                )
                .await
        },
    )
    .unwrap();
    assert!(result.await.unwrap().is_none());
    supervisor.shutdown().await;
    let transfer = DriveTransfer::new(
        Client::new(),
        v3.clone(),
        v3.clone(),
        v2,
        upload,
        tokio_util::sync::CancellationToken::new(),
        Arc::new(|_| {}),
    );
    let tracked = TrackedFileSync::new(transfer, history.clone());
    let listing = DriveListing::new(Client::new(), v3);
    let key = history.key(&save, "folder", "save.dat").unwrap();
    for _ in 0..2 {
        assert_eq!(
            reconcile_conditional_upload(&mut supervisor, &listing, &tracked, "synthetic")
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
    }
    assert_eq!(
        reconcile_conditional_upload(&mut supervisor, &listing, &tracked, "synthetic")
            .await
            .unwrap(),
        RecoveryStatus::Recovered
    );
    let checkpoint = history.get(&key).unwrap().unwrap();
    assert_eq!(checkpoint.remote_id, "old");
    assert_eq!(checkpoint.remote_version.as_deref(), Some("2"));
    assert_eq!(std::fs::read(&save).unwrap(), b"abc");
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_conditional_request_keeps_exclusion_until_reconciled() {
    use emusync_data_drive::DriveListing;
    use emusync_desktop_app::{
        AuthorizedTransferFailure, OperationSupervisor, RecoveryStatus, StartError,
        reconcile_conditional_upload, start_authorized_transfer,
    };
    use emusync_domain::sync::RemoteState;
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let v3 = Url::parse(&format!("{root}v3")).unwrap();
    let v2 = Url::parse(&format!("{root}v2")).unwrap();
    let upload = Url::parse(&format!("{root}upload")).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        request(&mut stream);
        let body = r#"{"id":"old","title":"save.dat","modifiedDate":"2026-01-01T00:00:00Z","etag":"\"v1\"","version":"1"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        let (_, body) = request(&mut stream);
        assert_eq!(body, b"abc");
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        drop(stream);
        let (mut stream, _) = listener.accept().unwrap();
        request(&mut stream);
        let body = r#"{"id":"old","name":"save.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","version":"2","sha256Checksum":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let credentials = synthetic_credentials(dir.path());
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let task_history = history.clone();
    let observed = RemoteState {
        id: "old".into(),
        modified: 1,
        sha256: None,
        version: Some("1".into()),
    };
    let path = save.to_str().unwrap().to_owned();
    let (task_v3, task_v2, task_upload) = (v3.clone(), v2.clone(), upload.clone());
    let mut supervisor =
        OperationSupervisor::<AuthorizedTransferFailure<emusync_data_local::ConfigManager>>::new();
    let result = start_authorized_transfer(
        &mut supervisor,
        credentials,
        move |cancel, guard| async move {
            let transfer = DriveTransfer::new(
                Client::new(),
                task_v3.clone(),
                task_v3,
                task_v2,
                task_upload,
                cancel,
                Arc::new(|_| {}),
            );
            TrackedFileSync::new(transfer, task_history)
                .upload_guarded(
                    "synthetic",
                    &path,
                    Some(&observed),
                    "folder",
                    "save.dat",
                    guard,
                )
                .await
        },
    )
    .unwrap();
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    supervisor.cancel();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), result)
            .await
            .unwrap()
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        supervisor.start(|_| async {
            Ok::<_, AuthorizedTransferFailure<emusync_data_local::ConfigManager>>(())
        }),
        Err(StartError::Busy)
    ));
    release_tx.send(()).unwrap();
    let transfer = DriveTransfer::new(
        Client::new(),
        v3.clone(),
        v3.clone(),
        v2,
        upload,
        tokio_util::sync::CancellationToken::new(),
        Arc::new(|_| {}),
    );
    let tracked = TrackedFileSync::new(transfer, history.clone());
    let listing = DriveListing::new(Client::new(), v3);
    assert_eq!(
        reconcile_conditional_upload(&mut supervisor, &listing, &tracked, "synthetic")
            .await
            .unwrap(),
        RecoveryStatus::Recovered
    );
    assert!(
        history
            .get(&history.key(&save, "folder", "save.dat").unwrap())
            .unwrap()
            .is_some()
    );
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn created_file_with_invalid_timestamp_reconciles_by_identity_without_recreating() {
    use emusync_data_drive::DriveListing;
    use emusync_desktop_app::{
        AuthorizedTransferFailure, OperationSupervisor, RecoveryStatus, StartError,
        reconcile_created_upload, start_authorized_transfer,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let v3 = Url::parse(&format!("{root}v3")).unwrap();
    let v2 = Url::parse(&format!("{root}v2")).unwrap();
    let upload = Url::parse(&format!("{root}upload")).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, _) = request(&mut stream);
        assert!(headers.starts_with("POST /upload?"));
        write!(stream,"HTTP/1.1 200 OK\r\nLocation: /session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, body) = request(&mut stream);
        assert!(headers.starts_with("PUT /session"));
        assert_eq!(body, b"abc");
        let body =
            r#"{"id":"created","name":"save.dat","parents":["folder"],"modifiedTime":"invalid"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, _) = request(&mut stream);
        assert!(headers.starts_with("GET /v3/created?"));
        let body = r#"{"id":"created","name":"save.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","version":"1","sha256Checksum":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let credentials = synthetic_credentials(dir.path());
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let task_history = history.clone();
    let path = save.to_str().unwrap().to_owned();
    let (task_v3, task_v2, task_upload) = (v3.clone(), v2.clone(), upload.clone());
    let mut supervisor =
        OperationSupervisor::<AuthorizedTransferFailure<emusync_data_local::ConfigManager>>::new();
    let result = start_authorized_transfer(
        &mut supervisor,
        credentials,
        move |cancel, guard| async move {
            let transfer = DriveTransfer::new(
                Client::new(),
                task_v3,
                task_upload.clone(),
                task_v2,
                task_upload,
                cancel,
                Arc::new(|_| {}),
            );
            TrackedFileSync::new(transfer, task_history)
                .upload_guarded("synthetic", &path, None, "folder", "save.dat", guard)
                .await
        },
    )
    .unwrap();
    assert!(result.await.unwrap().is_none());
    assert!(matches!(
        supervisor.start(|_| async {
            Ok::<_, AuthorizedTransferFailure<emusync_data_local::ConfigManager>>(())
        }),
        Err(StartError::Busy)
    ));
    let transfer = DriveTransfer::new(
        Client::new(),
        v3.clone(),
        upload.clone(),
        v2,
        upload,
        tokio_util::sync::CancellationToken::new(),
        Arc::new(|_| {}),
    );
    let tracked = TrackedFileSync::new(transfer, history.clone());
    assert_eq!(
        reconcile_created_upload(
            &mut supervisor,
            &DriveListing::new(Client::new(), v3),
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
async fn updated_file_with_invalid_timestamp_reconciles_against_observed_id() {
    use emusync_data_drive::{DriveListing, TransferError, UploadedFile};
    use emusync_data_local::{TrackedError, UploadContext, UploadSnapshot};
    use emusync_desktop_app::{
        AuthorizedTransferFailure, OperationSupervisor, RecoveryStatus,
        reconcile_conditional_upload, start_authorized_transfer,
    };
    use emusync_domain::sync::RemoteState;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, _) = request(&mut stream);
        assert!(headers.starts_with("GET /files/old?"));
        let body = r#"{"id":"old","name":"save.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","version":"2","sha256Checksum":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let credentials = synthetic_credentials(dir.path());
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let snapshot = UploadSnapshot::prepare(&save).unwrap();
    let context = UploadContext {
        local: save.clone(),
        folder: "folder".into(),
        remote_name: "save.dat".into(),
        observed: Some(RemoteState {
            id: "old".into(),
            modified: 1,
            sha256: None,
            version: Some("1".into()),
        }),
        recovery_id: None,
    };
    let uploaded: UploadedFile =
        serde_json::from_str(r#"{"id":"old","title":"save.dat","modifiedDate":"invalid"}"#)
            .unwrap();
    let mut supervisor =
        OperationSupervisor::<AuthorizedTransferFailure<emusync_data_local::ConfigManager>>::new();
    let result = start_authorized_transfer(&mut supervisor, credentials, move |_, _| async move {
        Err::<(), _>(TrackedError::Upload {
            source: TransferError::InvalidUpdatedTimestamp(Box::new(uploaded)),
            snapshot,
            context: Box::new(context),
        })
    })
    .unwrap();
    assert!(result.await.unwrap().is_none());
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let tracked = TrackedFileSync::new((), history.clone());
    assert_eq!(
        reconcile_conditional_upload(
            &mut supervisor,
            &DriveListing::new(Client::new(), endpoint),
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
            .remote_version
            .as_deref(),
        Some("2")
    );
    server.join().unwrap();
}

#[tokio::test]
async fn stale_credentials_prevent_recovery_metadata_request() {
    use emusync_data_drive::{CreatedFile, DriveListing, TransferError};
    use emusync_data_local::{
        ConfigManager, RecoveryCredentials, TrackedError, UploadContext, UploadSnapshot,
    };
    use emusync_desktop_app::{
        AuthorizedTransferFailure, OperationSupervisor, RecoveryError, reconcile_created_upload,
        start_authorized_transfer,
    };
    use emusync_domain::config::{AppConfig, ConfigurationStore, CredentialStore, Credentials};
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
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let snapshot = UploadSnapshot::prepare(&save).unwrap();
    let context = UploadContext {
        local: save,
        folder: "folder".into(),
        remote_name: "save.dat".into(),
        observed: None,
        recovery_id: None,
    };
    let file = CreatedFile {
        id: "created".into(),
        name: "save.dat".into(),
        modified_time: "invalid".into(),
        sha256_checksum: None,
        version: None,
        parents: vec!["folder".into()],
    };
    let mut supervisor = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let result = start_authorized_transfer(&mut supervisor, guard, move |_, _| async move {
        Err::<(), _>(TrackedError::Upload {
            source: TransferError::InvalidCreatedTimestamp(Box::new(file)),
            snapshot,
            context: Box::new(context),
        })
    })
    .unwrap();
    assert!(result.await.unwrap().is_none());
    store.update_credentials(|_| None).unwrap();
    store
        .update_credentials(|_| config.credentials.clone())
        .unwrap();
    let listing = DriveListing::new(
        Client::new(),
        Url::parse("http://127.0.0.1:1/files").unwrap(),
    );
    let tracked = TrackedFileSync::new(
        (),
        Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap()),
    );
    assert!(
        matches!(reconcile_created_upload(&mut supervisor, &listing, &tracked, "synthetic").await, Err(RecoveryError::Local(error)) if error.kind() == std::io::ErrorKind::PermissionDenied)
    );
}

#[tokio::test]
async fn recovery_rejects_reconnected_credentials_and_retains_snapshot_and_exclusion() {
    use emusync_data_drive::ResumableUpload;
    use emusync_data_local::{
        ConfigManager, RecoveryCredentials, TrackedError, UploadContext, UploadSnapshot,
    };
    use emusync_desktop_app::{
        AuthorizedTransferFailure, OperationSupervisor, RecoveryError, StartError,
        reconcile_tracked_upload, start_authorized_transfer,
    };
    use emusync_domain::{
        config::{AppConfig, ConfigurationStore, CredentialStore, Credentials},
        sync::RemoteState,
    };
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
        matches!(result,Err(RecoveryError::Local(error)) if error.kind()==std::io::ErrorKind::PermissionDenied)
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
async fn stale_authorization_never_starts_transfer() {
    use emusync_data_local::{ConfigManager, RecoveryCredentials};
    use emusync_desktop_app::{
        AuthorizedTransferFailure, OperationFailure, OperationSupervisor, start_authorized_transfer,
    };
    use emusync_domain::config::{AppConfig, ConfigurationStore, CredentialStore, Credentials};
    use std::sync::atomic::{AtomicBool, Ordering};
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
        matches!(&*guard,OperationFailure::Operation(failure) if matches!(&failure.error,emusync_data_local::TrackedError::Local(error) if error.kind()==std::io::ErrorKind::PermissionDenied))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connection_changed_during_conditional_query_cannot_record_old_revision() {
    use emusync_data_drive::DriveListing;
    use emusync_data_local::{ConfigManager, RecoveryCredentials};
    use emusync_desktop_app::{
        AuthorizedTransferFailure, OperationSupervisor, RecoveryError, StartError,
        reconcile_conditional_upload, start_authorized_transfer,
    };
    use emusync_domain::{
        config::{AppConfig, ConfigurationStore, CredentialStore, Credentials},
        sync::RemoteState,
    };
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
    let credentials = RecoveryCredentials::new(
        manager.clone(),
        manager.snapshot(&config).unwrap(),
        config.clone(),
    );
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let v3 = Url::parse(&format!("{root}v3")).unwrap();
    let v2 = Url::parse(&format!("{root}v2")).unwrap();
    let upload = Url::parse(&format!("{root}upload")).unwrap();
    let changing = manager.clone();
    let original = config.credentials.clone();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        request(&mut s);
        let body = r#"{"id":"old","title":"save.dat","modifiedDate":"2026-01-01T00:00:00Z","etag":"\"v1\"","version":"1"}"#;
        write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        let (mut s, _) = listener.accept().unwrap();
        let (_, body) = request(&mut s);
        assert_eq!(body, b"abc");
        drop(s);
        let (mut s, _) = listener.accept().unwrap();
        let (headers, _) = request(&mut s);
        assert!(headers.starts_with("GET /v3/old?"));
        changing.update_credentials(|_| None).unwrap();
        changing.update_credentials(|_| original).unwrap();
        let body = r#"{"id":"old","name":"save.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","version":"2","sha256Checksum":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}"#;
        write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
    });
    let observed = RemoteState {
        id: "old".into(),
        modified: 1_700_000_000_000_000_000,
        sha256: None,
        version: Some("1".into()),
    };
    let task_history = history.clone();
    let task_path = save.to_str().unwrap().to_owned();
    let task_v3 = v3.clone();
    let task_v2 = v2.clone();
    let task_upload = upload.clone();
    let mut supervisor = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let result = start_authorized_transfer(
        &mut supervisor,
        credentials,
        move |cancel, operation_guard| async move {
            let transfer = DriveTransfer::new(
                Client::new(),
                task_v3.clone(),
                task_v3,
                task_v2,
                task_upload,
                cancel,
                Arc::new(|_| {}),
            );
            TrackedFileSync::new(transfer, task_history)
                .upload_guarded(
                    "synthetic",
                    &task_path,
                    Some(&observed),
                    "folder",
                    "save.dat",
                    operation_guard,
                )
                .await
        },
    )
    .unwrap();
    assert!(result.await.unwrap().is_none());
    supervisor.shutdown().await;
    let transfer = DriveTransfer::new(
        Client::new(),
        v3.clone(),
        v3.clone(),
        v2,
        upload,
        tokio_util::sync::CancellationToken::new(),
        Arc::new(|_| {}),
    );
    let tracked = TrackedFileSync::new(transfer, history.clone());
    let listing = DriveListing::new(Client::new(), v3);
    let result =
        reconcile_conditional_upload(&mut supervisor, &listing, &tracked, "synthetic").await;
    assert!(
        matches!(result,Err(RecoveryError::Local(error)) if error.kind()==std::io::ErrorKind::PermissionDenied)
    );
    let key = history.key(&save, "folder", "save.dat").unwrap();
    assert!(history.get(&key).unwrap().is_none());
    assert_eq!(std::fs::read(&save).unwrap(), b"abc");
    assert!(matches!(
        supervisor.start(|_| async { Ok::<_, AuthorizedTransferFailure<ConfigManager>>(()) }),
        Err(StartError::Busy)
    ));
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connection_change_during_successful_upload_cannot_record_old_history() {
    use emusync_data_local::{ConfigManager, RecoveryCredentials, TrackedError};
    use emusync_desktop_app::{
        AuthorizedTransferFailure, OperationFailure, OperationSupervisor, start_authorized_transfer,
    };
    use emusync_domain::config::{AppConfig, ConfigurationStore, CredentialStore, Credentials};
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
        request(&mut s);
        write!(s,"HTTP/1.1 200 OK\r\nLocation: /session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        let (mut s, _) = listener.accept().unwrap();
        let (_, body) = request(&mut s);
        assert_eq!(body, b"abc");
        changing.update_credentials(|_| None).unwrap();
        changing.update_credentials(|_| original).unwrap();
        let body = r#"{"id":"created","name":"save.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","sha256Checksum":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad","version":"2"}"#;
        write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
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
        matches!(&*locked,OperationFailure::Operation(failure) if matches!(&failure.error,TrackedError::AfterUpload { source,remote,.. } if source.kind()==std::io::ErrorKind::PermissionDenied && remote.id=="created"))
    );
    let key = history.key(&save, "folder", "save.dat").unwrap();
    assert!(history.get(&key).unwrap().is_none());
    assert_eq!(std::fs::read(&save).unwrap(), b"abc");
    server.join().unwrap();
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn changed_connection_during_pinned_download_preserves_save_and_history() {
    use emusync_data_local::{ConfigManager, RecoveryCredentials, TrackedError};
    use emusync_desktop_app::{
        AuthorizedTransferFailure, OperationFailure, OperationSupervisor, start_authorized_transfer,
    };
    use emusync_domain::{
        config::{AppConfig, ConfigurationStore, CredentialStore, Credentials},
        sync::RemoteState,
    };
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
        let (headers, _) = request(&mut s);
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
        matches!(&*locked,OperationFailure::Operation(failure) if matches!(&failure.error,TrackedError::Local(error) if error.kind()==std::io::ErrorKind::PermissionDenied))
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
    use emusync_data_drive::ResumableUpload;
    use emusync_data_local::ConfigManager;
    use emusync_desktop_app::{
        AuthorizedTransferFailure, CloseState, RecoveryStatus, RuntimeOwner,
        reconcile_tracked_upload, start_authorized_transfer,
    };
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };
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
        request(&mut s);
        write!(s,"HTTP/1.1 200 OK\r\nLocation: /session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        let (mut s, _) = listener.accept().unwrap();
        let (_, body) = request(&mut s);
        assert_eq!(body, b"abc");
        put_tx.send(()).unwrap();
        put_release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(s);
        let (mut s, _) = listener.accept().unwrap();
        let (headers, _) = request(&mut s);
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
        write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
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
