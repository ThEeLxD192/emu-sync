mod common;

use common::{read_http_request, synthetic_credentials};
use emusync_data_drive::{
    ConditionalError, CreatedFile, DriveListing, DriveTransfer, TransferError, UploadedFile,
};
use emusync_data_local::{
    ConfigManager, RecoveryCredentials, SyncHistoryStore, TrackedError, TrackedFileSync,
    UploadContext, UploadSnapshot,
};
use emusync_desktop_app::{
    AuthorizedTransferFailure, OperationFailure, OperationSupervisor, RecoveryError,
    RecoveryStatus, StartError, reconcile_conditional_upload, reconcile_created_upload,
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
async fn uncertain_conditional_write_reconciles_only_after_matching_hash() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let (v3, v2, upload) = (
        Url::parse(&format!("{root}v3")).unwrap(),
        Url::parse(&format!("{root}v2")).unwrap(),
        Url::parse(&format!("{root}upload")).unwrap(),
    );
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_http_request(&mut s).unwrap();
        let b = r#"{"id":"old","title":"save.dat","modifiedDate":"2026-01-01T00:00:00Z","etag":"\"v1\"","version":"1"}"#;
        write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}", b.len()).unwrap();
        let (mut s, _) = listener.accept().unwrap();
        let (h, body) = read_http_request(&mut s).unwrap();
        assert!(h.starts_with("PUT /upload/old?"));
        assert!(h.to_ascii_lowercase().contains("if-match: \"v1\""));
        assert_eq!(body, b"abc");
        drop(s);
        for n in 0..3 {
            let (mut s, _) = listener.accept().unwrap();
            assert!(
                read_http_request(&mut s)
                    .unwrap()
                    .0
                    .starts_with("GET /v3/old?")
            );
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
            write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let credentials = synthetic_credentials(dir.path());
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let observed = RemoteState {
        id: "old".into(),
        modified: 1_700_000_000_000_000_000,
        sha256: None,
        version: Some("1".into()),
    };
    let (t_hist, path, t_v3, t_v2, t_up) = (
        history.clone(),
        save.to_str().unwrap().to_owned(),
        v3.clone(),
        v2.clone(),
        upload.clone(),
    );
    let mut supervisor = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let result = start_authorized_transfer(
        &mut supervisor,
        credentials,
        move |cancel, guard| async move {
            let transfer = DriveTransfer::new(
                Client::new(),
                t_v3.clone(),
                t_v3,
                t_v2,
                t_up,
                cancel,
                Arc::new(|_| {}),
            );
            TrackedFileSync::new(transfer, t_hist)
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
    for _ in 0..2 {
        assert_eq!(
            reconcile_conditional_upload(&mut supervisor, &listing, &tracked, "synthetic")
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
        reconcile_conditional_upload(&mut supervisor, &listing, &tracked, "synthetic")
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_conditional_request_keeps_exclusion_until_reconciled() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let (v3, v2, upload) = (
        Url::parse(&format!("{root}v3")).unwrap(),
        Url::parse(&format!("{root}v2")).unwrap(),
        Url::parse(&format!("{root}upload")).unwrap(),
    );
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_http_request(&mut s).unwrap();
        let b = r#"{"id":"old","title":"save.dat","modifiedDate":"2026-01-01T00:00:00Z","etag":"\"v1\"","version":"1"}"#;
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
            b.len()
        )
        .unwrap();
        let (mut s, _) = listener.accept().unwrap();
        assert_eq!(read_http_request(&mut s).unwrap().1, b"abc");
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        drop(s);
        let (mut s, _) = listener.accept().unwrap();
        read_http_request(&mut s).unwrap();
        let b = r#"{"id":"old","name":"save.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","version":"2","sha256Checksum":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}"#;
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
            b.len()
        )
        .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let credentials = synthetic_credentials(dir.path());
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let observed = RemoteState {
        id: "old".into(),
        modified: 1,
        sha256: None,
        version: Some("1".into()),
    };
    let (t_hist, path, t_v3, t_v2, t_up) = (
        history.clone(),
        save.to_str().unwrap().to_owned(),
        v3.clone(),
        v2.clone(),
        upload.clone(),
    );
    let mut supervisor = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let result = start_authorized_transfer(
        &mut supervisor,
        credentials,
        move |cancel, guard| async move {
            let transfer = DriveTransfer::new(
                Client::new(),
                t_v3.clone(),
                t_v3,
                t_v2,
                t_up,
                cancel,
                Arc::new(|_| {}),
            );
            TrackedFileSync::new(transfer, t_hist)
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
    assert!(result.await.unwrap().is_none());
    release_tx.send(()).unwrap();
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
    assert_eq!(
        reconcile_conditional_upload(&mut supervisor, &listing, &tracked, "synthetic")
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn created_file_with_invalid_timestamp_reconciles_by_identity_without_recreating() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let (v3, v2, upload) = (
        Url::parse(&format!("{root}v3")).unwrap(),
        Url::parse(&format!("{root}v2")).unwrap(),
        Url::parse(&format!("{root}upload")).unwrap(),
    );
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_http_request(&mut s).unwrap();
        write!(s, "HTTP/1.1 200 OK\r\nLocation: /session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        let (mut s, _) = listener.accept().unwrap();
        assert_eq!(read_http_request(&mut s).unwrap().1, b"abc");
        let b1 =
            r#"{"id":"created","name":"save.dat","parents":["folder"],"modifiedTime":"invalid"}"#;
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
                .starts_with("GET /v3/created?")
        );
        let b2 = r#"{"id":"created","name":"save.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","version":"1","sha256Checksum":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}"#;
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b2}",
            b2.len()
        )
        .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let credentials = synthetic_credentials(dir.path());
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let (t_hist, path, t_v3, t_v2, t_up) = (
        history.clone(),
        save.to_str().unwrap().to_owned(),
        v3.clone(),
        v2.clone(),
        upload.clone(),
    );
    let mut supervisor = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let result = start_authorized_transfer(
        &mut supervisor,
        credentials,
        move |cancel, guard| async move {
            let transfer = DriveTransfer::new(
                Client::new(),
                t_v3,
                t_up.clone(),
                t_v2,
                t_up,
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
    assert!(matches!(
        supervisor.start(|_| async { Ok::<_, AuthorizedTransferFailure<ConfigManager>>(()) }),
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
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        assert!(
            read_http_request(&mut s)
                .unwrap()
                .0
                .starts_with("GET /files/old?")
        );
        let b = r#"{"id":"old","name":"save.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","version":"2","sha256Checksum":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}"#;
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
            b.len()
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
    let mut supervisor = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
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
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(ConfigManager::new(&dir.path().join("config.json")).unwrap());
    let config = AppConfig {
        language: Default::default(),
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
        matches!(reconcile_created_upload(&mut supervisor, &listing, &tracked, "synthetic").await, Err(RecoveryError::Local(e)) if e.kind() == std::io::ErrorKind::PermissionDenied)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connection_changed_during_conditional_query_cannot_record_old_revision() {
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
    let (v3, v2, upload) = (
        Url::parse(&format!("{root}v3")).unwrap(),
        Url::parse(&format!("{root}v2")).unwrap(),
        Url::parse(&format!("{root}upload")).unwrap(),
    );
    let changing = manager.clone();
    let original = config.credentials.clone();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_http_request(&mut s).unwrap();
        let b = r#"{"id":"old","title":"save.dat","modifiedDate":"2026-01-01T00:00:00Z","etag":"\"v1\"","version":"1"}"#;
        write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}", b.len()).unwrap();
        let (mut s, _) = listener.accept().unwrap();
        assert_eq!(read_http_request(&mut s).unwrap().1, b"abc");
        drop(s);
        let (mut s, _) = listener.accept().unwrap();
        assert!(
            read_http_request(&mut s)
                .unwrap()
                .0
                .starts_with("GET /v3/old?")
        );
        changing.update_credentials(|_| None).unwrap();
        changing.update_credentials(|_| original).unwrap();
        let b = r#"{"id":"old","name":"save.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","version":"2","sha256Checksum":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}"#;
        write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}", b.len()).unwrap();
    });
    let observed = RemoteState {
        id: "old".into(),
        modified: 1_700_000_000_000_000_000,
        sha256: None,
        version: Some("1".into()),
    };
    let (t_hist, path, t_v3, t_v2, t_up) = (
        history.clone(),
        save.to_str().unwrap().to_owned(),
        v3.clone(),
        v2.clone(),
        upload.clone(),
    );
    let mut supervisor = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let result = start_authorized_transfer(
        &mut supervisor,
        credentials,
        move |cancel, guard| async move {
            let transfer = DriveTransfer::new(
                Client::new(),
                t_v3.clone(),
                t_v3,
                t_v2,
                t_up,
                cancel,
                Arc::new(|_| {}),
            );
            TrackedFileSync::new(transfer, t_hist)
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
        matches!(result, Err(RecoveryError::Local(e)) if e.kind() == std::io::ErrorKind::PermissionDenied)
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

#[tokio::test]
async fn reconcile_conditional_upload_rejects_missing_observed_state_or_created_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = Arc::new(ConfigManager::new(&config_path).unwrap());
    let config = AppConfig {
        language: Default::default(),
        credentials: Some(Credentials {
            client_id: "client".into(),
            client_secret: "synthetic".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![],
    };
    manager.save(&config).unwrap();
    let credentials =
        RecoveryCredentials::new(manager.clone(), manager.snapshot(&config).unwrap(), config);
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let listing = DriveListing::new(
        Client::new(),
        Url::parse("http://127.0.0.1:1/files").unwrap(),
    );
    let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
    let tracked = TrackedFileSync::new((), history);

    // Case 1: Conditional upload with observed: None
    let mut sup1 = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let snap1 = UploadSnapshot::prepare(&save).unwrap();
    let ctx1 = UploadContext {
        local: save.clone(),
        folder: "folder".into(),
        remote_name: "save.dat".into(),
        observed: None,
        recovery_id: None,
    };
    let f1 = AuthorizedTransferFailure::new(
        TrackedError::Upload {
            source: TransferError::Conditional(ConditionalError::HeadersTimedOut),
            snapshot: snap1,
            context: Box::new(ctx1),
        },
        credentials.clone(),
    );
    assert!(sup1.retain_failure(OperationFailure::Operation(f1)));
    assert!(matches!(
        reconcile_conditional_upload(&mut sup1, &listing, &tracked, "synthetic").await,
        Err(RecoveryError::UnsupportedFailure)
    ));
    assert!(sup1.failure().is_some());

    // Case 2a: Created upload with empty file.id
    let mut sup2 = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let snap2 = UploadSnapshot::prepare(&save).unwrap();
    let ctx2 = UploadContext {
        local: save.clone(),
        folder: "folder".into(),
        remote_name: "save.dat".into(),
        observed: None,
        recovery_id: None,
    };
    let file2 = CreatedFile {
        id: "   ".into(),
        name: "save.dat".into(),
        modified_time: "invalid".into(),
        sha256_checksum: None,
        version: None,
        parents: vec![],
    };
    let f2 = AuthorizedTransferFailure::new(
        TrackedError::Upload {
            source: TransferError::InvalidCreatedTimestamp(Box::new(file2)),
            snapshot: snap2,
            context: Box::new(ctx2),
        },
        credentials.clone(),
    );
    assert!(sup2.retain_failure(OperationFailure::Operation(f2)));
    assert!(matches!(
        reconcile_created_upload(&mut sup2, &listing, &tracked, "synthetic").await,
        Err(RecoveryError::UnsupportedFailure)
    ));
    assert!(sup2.failure().is_some());

    // Case 2b: Created upload with mismatched file.name
    let mut sup3 = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let snap3 = UploadSnapshot::prepare(&save).unwrap();
    let ctx3 = UploadContext {
        local: save.clone(),
        folder: "folder".into(),
        remote_name: "save.dat".into(),
        observed: None,
        recovery_id: None,
    };
    let file3 = CreatedFile {
        id: "id-1".into(),
        name: "wrong.dat".into(),
        modified_time: "invalid".into(),
        sha256_checksum: None,
        version: None,
        parents: vec![],
    };
    let f3 = AuthorizedTransferFailure::new(
        TrackedError::Upload {
            source: TransferError::InvalidCreatedTimestamp(Box::new(file3)),
            snapshot: snap3,
            context: Box::new(ctx3),
        },
        credentials.clone(),
    );
    assert!(sup3.retain_failure(OperationFailure::Operation(f3)));
    assert!(matches!(
        reconcile_created_upload(&mut sup3, &listing, &tracked, "synthetic").await,
        Err(RecoveryError::UnsupportedFailure)
    ));
    assert!(sup3.failure().is_some());

    // Case 2c: Created upload with observed: Some(...)
    let mut sup4 = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let snap4 = UploadSnapshot::prepare(&save).unwrap();
    let ctx4 = UploadContext {
        local: save,
        folder: "folder".into(),
        remote_name: "save.dat".into(),
        observed: Some(RemoteState {
            id: "old".into(),
            modified: 1,
            sha256: None,
            version: None,
        }),
        recovery_id: None,
    };
    let file4 = CreatedFile {
        id: "id-1".into(),
        name: "save.dat".into(),
        modified_time: "invalid".into(),
        sha256_checksum: None,
        version: None,
        parents: vec![],
    };
    let f4 = AuthorizedTransferFailure::new(
        TrackedError::Upload {
            source: TransferError::InvalidCreatedTimestamp(Box::new(file4)),
            snapshot: snap4,
            context: Box::new(ctx4),
        },
        credentials,
    );
    assert!(sup4.retain_failure(OperationFailure::Operation(f4)));
    assert!(matches!(
        reconcile_created_upload(&mut sup4, &listing, &tracked, "synthetic").await,
        Err(RecoveryError::UnsupportedFailure)
    ));
    assert!(sup4.failure().is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconcile_conditional_upload_rejects_remote_folder_or_renamed_file_with_matching_digest() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        // Attempt 1: mimeType is folder with matching SHA-256
        let (mut s, _) = listener.accept().unwrap();
        assert!(
            read_http_request(&mut s)
                .unwrap()
                .0
                .starts_with("GET /files/old?")
        );
        let b1 = r#"{"id":"old","name":"save.dat","mimeType":"application/vnd.google-apps.folder","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","version":"2","sha256Checksum":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}"#;
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b1}",
            b1.len()
        )
        .unwrap();

        // Attempt 2: wrong name ("wrong_name.dat") with matching SHA-256
        let (mut s, _) = listener.accept().unwrap();
        assert!(
            read_http_request(&mut s)
                .unwrap()
                .0
                .starts_with("GET /files/old?")
        );
        let b2 = r#"{"id":"old","name":"wrong_name.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","version":"2","sha256Checksum":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}"#;
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b2}",
            b2.len()
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
    let mut supervisor = OperationSupervisor::<AuthorizedTransferFailure<ConfigManager>>::new();
    let failure = AuthorizedTransferFailure::new(
        TrackedError::Upload {
            source: TransferError::Conditional(ConditionalError::HeadersTimedOut),
            snapshot,
            context: Box::new(context),
        },
        credentials,
    );
    assert!(supervisor.retain_failure(OperationFailure::Operation(failure)));

    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let tracked = TrackedFileSync::new((), history.clone());
    let listing = DriveListing::new(Client::new(), endpoint);

    for _ in 0..2 {
        let status = reconcile_conditional_upload(&mut supervisor, &listing, &tracked, "synthetic")
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
