mod common;

use common::{read_request, reply};
use emusync_data_drive::DriveTransfer;
use emusync_data_local::{SyncHistoryStore, TrackedFileSync};
use emusync_domain::sync::LocalSaveFiles;
use reqwest::{Client, Url};
use std::{io::Write, net::TcpListener, sync::Arc, thread};
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn empty_conditional_upload_timeout_retains_snapshot_and_history() {
    use emusync_data_drive::{ConditionalError, DriveHttpPolicy, TransferError};
    use emusync_data_local::TrackedError;
    use emusync_domain::sync::RemoteState;
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        let body = r#"{"id":"old","title":"save.dat","modifiedDate":"2026-01-01T00:00:00Z","etag":"\"v1\"","version":"1"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut stream);
        assert!(headers.to_ascii_lowercase().contains("if-match: \"v1\""));
        assert!(body.is_empty());
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("save.dat");
    std::fs::write(&save, []).unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let clients = DriveHttpPolicy {
        connect: Duration::from_secs(2),
        metadata: Duration::from_secs(2),
        inactivity: Duration::from_millis(200),
    }
    .build()
    .unwrap();
    let transfer = DriveTransfer::with_clients(
        clients,
        base.clone(),
        base.clone(),
        base.clone(),
        base,
        CancellationToken::new(),
        Arc::new(|_| {}),
    );
    let task_history = history.clone();
    let path = save.to_str().unwrap().to_owned();
    let task = tokio::spawn(async move {
        TrackedFileSync::new(transfer, task_history)
            .upload(
                "synthetic",
                &path,
                Some(&RemoteState {
                    id: "old".into(),
                    modified: 1,
                    sha256: None,
                    version: Some("1".into()),
                }),
                "folder",
                "save.dat",
            )
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap();
    match result {
        Err(TrackedError::Upload {
            source:
                TransferError::Conditional(
                    ConditionalError::OutcomeUnknown(_) | ConditionalError::HeadersTimedOut,
                ),
            snapshot,
            ..
        }) => {
            assert_eq!(std::fs::read(snapshot.path()).unwrap(), b"");
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(
        history
            .get(&history.key(&save, "folder", "save.dat").unwrap())
            .unwrap()
            .is_none()
    );
    assert_eq!(std::fs::read(&save).unwrap(), b"");
    release_tx.send(()).unwrap();
    server.join().unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tracked_upload_uses_snapshot_and_records_server_revision() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let base = Url::parse(&format!("http://{address}/files")).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut s);
        assert!(headers.starts_with("POST /files?"));
        let metadata: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(metadata["name"], "save.dat");
        assert_eq!(metadata["parents"][0], "folder");
        write!(s,"HTTP/1.1 200 OK\r\nLocation: /session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        let (mut s, _) = listener.accept().unwrap();
        let (_, body) = read_request(&mut s);
        assert_eq!(body, b"abc");
        let body = r#"{"id":"new-id","name":"save.dat","modifiedTime":"2026-01-01T00:00:00Z","sha256Checksum":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad","version":"7"}"#;
        write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let transfer = DriveTransfer::new(
        Client::new(),
        base.clone(),
        base.clone(),
        base.clone(),
        base,
        CancellationToken::new(),
        Arc::new(|_| {}),
    );
    let tracked = TrackedFileSync::new(transfer, history.clone());
    tracked
        .upload(
            "synthetic",
            save.to_str().unwrap(),
            None,
            "folder",
            "save.dat",
        )
        .await
        .unwrap();
    let key = history.key(&save, "folder", "save.dat").unwrap();
    let checkpoint = history.get(&key).unwrap().unwrap();
    assert_eq!(checkpoint.remote_id, "new-id");
    assert_eq!(checkpoint.remote_version.as_deref(), Some("7"));
    assert_eq!(
        checkpoint.local_sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(std::fs::read(&save).unwrap(), b"abc");
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_tracked_download_waits_for_staging_cleanup_and_keeps_save() {
    use emusync_domain::sync::RemoteState;
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let base = Url::parse(&format!("http://{address}/files")).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let (headers, _) = read_request(&mut s);
        assert!(headers.starts_with("GET /files/remote?"));
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\npartial"
        )
        .unwrap();
        s.flush().unwrap();
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"original").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let cancel = CancellationToken::new();
    let transfer = DriveTransfer::new(
        Client::new(),
        base.clone(),
        base.clone(),
        base.clone(),
        base,
        cancel.clone(),
        Arc::new(|_| {}),
    );
    let tracked = TrackedFileSync::new(transfer, history.clone());
    let path = save.to_str().unwrap().to_owned();
    let remote = RemoteState {
        id: "remote".into(),
        modified: 1_700_000_000_000_000_000,
        sha256: None,
        version: None,
    };
    let task = tokio::spawn(async move {
        tracked
            .download("synthetic", &remote, &path, "folder", "save.dat")
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let staged = std::fs::read_dir(dir.path())
                .unwrap()
                .filter_map(Result::ok)
                .any(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with(".emusync-download-")
                        // Not `e.metadata()`: on Windows the directory entry keeps a stale
                        // size while the staging file is open for writing.
                        && std::fs::metadata(e.path()).is_ok_and(|m| m.len() == 7)
                });
            if staged {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert!(result.is_err());
    assert_eq!(std::fs::read(&save).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    assert!(
        history
            .get(&history.key(&save, "folder", "save.dat").unwrap())
            .unwrap()
            .is_none()
    );
    release_tx.send(()).unwrap();
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interrupted_upload_retains_snapshot_and_session_for_reconciliation() {
    use emusync_data_drive::{TransferError, UploadError};
    use emusync_data_local::TrackedError;
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        write!(s,"HTTP/1.1 200 OK\r\nLocation: /session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        let (mut s, _) = listener.accept().unwrap();
        let (_, body) = read_request(&mut s);
        assert_eq!(body, b"abc");
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let cancel = CancellationToken::new();
    let transfer = DriveTransfer::new(
        Client::new(),
        base.clone(),
        base.clone(),
        base.clone(),
        base,
        cancel.clone(),
        Arc::new(|_| {}),
    );
    let tracked = TrackedFileSync::new(transfer, history.clone());
    let path = save.to_str().unwrap().to_owned();
    let task = tokio::spawn(async move {
        tracked
            .upload("synthetic", &path, None, "folder", "save.dat")
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    std::fs::write(&save, b"new").unwrap();
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    let (session, snapshot) = match result {
        Err(TrackedError::Upload {
            source: TransferError::Upload(UploadError::OutcomeUnknown(session)),
            snapshot,
            ..
        }) => (session, snapshot),
        other => panic!("unexpected result: {other:?}"),
    };
    assert!(!format!("{session:?}").contains("session"));
    assert_eq!(std::fs::read(snapshot.path()).unwrap(), b"abc");
    assert_eq!(
        snapshot.digest(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(std::fs::read(&save).unwrap(), b"new");
    assert!(
        history
            .get(&history.key(&save, "folder", "save.dat").unwrap())
            .unwrap()
            .is_none()
    );
    let snapshot_dir = snapshot.directory().to_owned();
    drop(snapshot);
    assert!(!snapshot_dir.exists());
    release_tx.send(()).unwrap();
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_edit_after_remote_commit_retains_original_snapshot_and_remote_identity() {
    use emusync_data_local::TrackedError;
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        write!(s,"HTTP/1.1 200 OK\r\nLocation: /session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        let (mut s, _) = listener.accept().unwrap();
        let (_, body) = read_request(&mut s);
        assert_eq!(body, b"abc");
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let body = r#"{"id":"created-id","name":"save.dat","modifiedTime":"2026-01-01T00:00:00Z","sha256Checksum":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad","version":"7"}"#;
        write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let transfer = DriveTransfer::new(
        Client::new(),
        base.clone(),
        base.clone(),
        base.clone(),
        base,
        CancellationToken::new(),
        Arc::new(|_| {}),
    );
    let tracked = TrackedFileSync::new(transfer, history.clone());
    let path = save.to_str().unwrap().to_owned();
    let task = tokio::spawn(async move {
        tracked
            .upload("synthetic", &path, None, "folder", "save.dat")
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    std::fs::write(&save, b"new").unwrap();
    release_tx.send(()).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    let (snapshot, remote) = match result {
        Err(TrackedError::AfterUpload {
            snapshot, remote, ..
        }) => (snapshot, remote),
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(remote.id, "created-id");
    assert_eq!(std::fs::read(snapshot.path()).unwrap(), b"abc");
    assert_eq!(std::fs::read(&save).unwrap(), b"new");
    let key = history.key(&save, "folder", "save.dat").unwrap();
    assert_eq!(history.get(&key).unwrap().unwrap().remote_id, "created-id");
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_edit_during_download_is_preserved_and_not_recorded_as_synced() {
    use emusync_domain::sync::RemoteState;
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, _) = read_request(&mut stream);
        assert!(headers.starts_with("GET /files/remote?"));
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\na"
        )
        .unwrap();
        stream.flush().unwrap();
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        stream.write_all(b"bc").unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"original").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let transfer = DriveTransfer::new(
        Client::new(),
        base.clone(),
        base.clone(),
        base.clone(),
        base,
        CancellationToken::new(),
        Arc::new(|_| {}),
    );
    let tracked = TrackedFileSync::new(transfer, history.clone());
    let path = save.to_str().unwrap().to_owned();
    let remote = RemoteState {
        id: "remote".into(),
        modified: 1_700_000_000_000_000_000,
        sha256: Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into()),
        version: Some("1".into()),
    };
    let task = tokio::spawn(async move {
        tracked
            .download("synthetic", &remote, &path, "folder", "save.dat")
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    std::fs::write(&save, b"newer").unwrap();
    release_tx.send(()).unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    assert_eq!(std::fs::read(&save).unwrap(), b"newer");
    assert!(
        history
            .get(&history.key(&save, "folder", "save.dat").unwrap())
            .unwrap()
            .is_none()
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_remote_checksum_does_not_replace_save_or_record_history() {
    use emusync_domain::sync::RemoteState;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc"
        )
        .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"original").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let transfer = DriveTransfer::new(
        Client::new(),
        base.clone(),
        base.clone(),
        base.clone(),
        base,
        CancellationToken::new(),
        Arc::new(|_| {}),
    );
    let tracked = TrackedFileSync::new(transfer, history.clone());
    let remote = RemoteState {
        id: "remote".into(),
        modified: 1_700_000_000_000_000_000,
        sha256: Some("0000000000000000000000000000000000000000000000000000000000000000".into()),
        version: Some("1".into()),
    };
    assert!(
        tracked
            .download(
                "synthetic",
                &remote,
                save.to_str().unwrap(),
                "folder",
                "save.dat"
            )
            .await
            .is_err()
    );
    assert_eq!(std::fs::read(&save).unwrap(), b"original");
    assert!(
        history
            .get(&history.key(&save, "folder", "save.dat").unwrap())
            .unwrap()
            .is_none()
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    server.join().unwrap();
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pinned_tracked_download_rejects_parent_symlink_swap_without_recording_history() {
    use emusync_domain::sync::RemoteState;
    use std::{os::unix::fs::symlink, sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let (headers, _) = read_request(&mut s);
        assert!(headers.starts_with("GET /files/remote?"));
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\na"
        )
        .unwrap();
        s.flush().unwrap();
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        s.write_all(b"bc").unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    let outside = dir.path().join("outside");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::create_dir(root.join("slot")).unwrap();
    std::fs::write(root.join("slot/save.dat"), b"original").unwrap();
    std::fs::write(outside.join("save.dat"), b"outside").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let transfer = DriveTransfer::new(
        Client::new(),
        base.clone(),
        base.clone(),
        base.clone(),
        base,
        CancellationToken::new(),
        Arc::new(|_| {}),
    );
    let tracked = TrackedFileSync::new(transfer, history.clone());
    let root_arg = root.to_str().unwrap().to_owned();
    let remote = RemoteState {
        id: "remote".into(),
        modified: 1_700_000_000_000_000_000,
        sha256: Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into()),
        version: Some("1".into()),
    };
    let task = tokio::spawn(async move {
        tracked
            .download_pinned("synthetic", &remote, &root_arg, "folder", "slot/save.dat")
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    std::fs::rename(root.join("slot"), root.join("original-slot")).unwrap();
    symlink(&outside, root.join("slot")).unwrap();
    release_tx.send(()).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert!(result.is_err());
    assert_eq!(
        std::fs::read(root.join("original-slot/save.dat")).unwrap(),
        b"original"
    );
    assert_eq!(std::fs::read(outside.join("save.dat")).unwrap(), b"outside");
    let key = history
        .key(&root.join("slot/save.dat"), "folder", "slot/save.dat")
        .unwrap();
    assert!(history.get(&key).unwrap().is_none());
    assert!(!history.backup_directory().exists());
    server.join().unwrap();
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pinned_tracked_download_restores_backup_and_records_revision() {
    use emusync_domain::sync::RemoteState;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc"
        )
        .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(root.join("slot")).unwrap();
    let save = root.join("slot/save.dat");
    std::fs::write(&save, b"original").unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let transfer = DriveTransfer::new(
        Client::new(),
        base.clone(),
        base.clone(),
        base.clone(),
        base,
        CancellationToken::new(),
        Arc::new(|_| {}),
    );
    let tracked = TrackedFileSync::new(transfer, history.clone());
    let remote = RemoteState {
        id: "remote".into(),
        modified: 1_700_000_000_000_000_000,
        sha256: Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into()),
        version: Some("1".into()),
    };
    tracked
        .download_pinned(
            "synthetic",
            &remote,
            root.to_str().unwrap(),
            "folder",
            "slot/save.dat",
        )
        .await
        .unwrap();
    assert_eq!(std::fs::read(&save).unwrap(), b"abc");
    let key = history.key(&save, "folder", "slot/save.dat").unwrap();
    assert_eq!(history.get(&key).unwrap().unwrap().remote_id, "remote");
    let old_hash = "0682c5f2076f099c34cfdd15a9e063849ed437a49677e6fcc5b4198c76575be5";
    assert_eq!(
        std::fs::read(
            history
                .backup_directory()
                .join(key)
                .join(old_hash)
                .join("save.dat")
        )
        .unwrap(),
        b"original"
    );
    server.join().unwrap();
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pinned_tracked_download_creates_nested_directories_and_records_revision() {
    use emusync_domain::sync::RemoteState;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc"
        )
        .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let transfer = DriveTransfer::new(
        Client::new(),
        base.clone(),
        base.clone(),
        base.clone(),
        base,
        CancellationToken::new(),
        Arc::new(|_| {}),
    );
    let tracked = TrackedFileSync::new(transfer, history.clone());
    let remote = RemoteState {
        id: "remote".into(),
        modified: 1_700_000_000_000_000_000,
        sha256: Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into()),
        version: Some("1".into()),
    };
    tracked
        .download_pinned(
            "synthetic",
            &remote,
            root.to_str().unwrap(),
            "folder",
            "one/two/save.dat",
        )
        .await
        .unwrap();
    let save = root.join("one/two/save.dat");
    assert_eq!(std::fs::read(&save).unwrap(), b"abc");
    let key = history.key(&save, "folder", "one/two/save.dat").unwrap();
    assert_eq!(history.get(&key).unwrap().unwrap().remote_id, "remote");
    assert!(!history.backup_directory().exists());
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tracked_conditional_replace_with_invalid_updated_timestamp_retains_snapshot_and_preserves_old_history()
 {
    use emusync_data_drive::TransferError;
    use emusync_data_local::{HistoryCheckpoint, TrackedError};
    use emusync_domain::sync::RemoteState;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();

    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        let get_body = r#"{"id":"old-id","title":"save.dat","modifiedDate":"2026-01-01T00:00:00Z","version":"1","etag":"\"v1\""}"#;
        reply(&mut s, "200 OK", get_body);

        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut s);
        assert!(headers.starts_with("PUT /files/old-id?"));
        assert!(headers.to_ascii_lowercase().contains("if-match: \"v1\""));
        assert_eq!(body, b"updated-save");
        let put_body = r#"{"id":"old-id","title":"save.dat","modifiedDate":"invalid-date","version":"2","etag":"\"v2\""}"#;
        reply(&mut s, "200 OK", put_body);
    });

    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"updated-save").unwrap();

    let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
    let key = history.key(&save, "folder", "save.dat").unwrap();
    history
        .record(
            &key,
            HistoryCheckpoint {
                local_sha256: "initial-hash".into(),
                remote_id: "old-id".into(),
                remote_modified_time: "2026-01-01T00:00:00Z".into(),
                remote_sha256: None,
                remote_version: Some("1".into()),
            },
        )
        .unwrap();

    let transfer = DriveTransfer::new(
        Client::new(),
        base.clone(),
        base.clone(),
        base.clone(),
        base,
        CancellationToken::new(),
        Arc::new(|_| {}),
    );

    let tracked = TrackedFileSync::new(transfer, history.clone());
    let observed = RemoteState {
        id: "old-id".into(),
        modified: 1_700_000_000,
        sha256: None,
        version: Some("1".into()),
    };

    let result = tracked
        .upload(
            "synthetic",
            save.to_str().unwrap(),
            Some(&observed),
            "folder",
            "save.dat",
        )
        .await;

    match result {
        Err(TrackedError::Upload {
            source: TransferError::InvalidUpdatedTimestamp(uploaded),
            snapshot,
            ..
        }) => {
            assert_eq!(uploaded.id, "old-id");
            assert_eq!(std::fs::read(snapshot.path()).unwrap(), b"updated-save");
        }
        other => panic!("expected TrackedError::Upload(InvalidUpdatedTimestamp), got: {other:?}"),
    }

    let checkpoint = history.get(&key).unwrap().unwrap();
    assert_eq!(checkpoint.remote_version.as_deref(), Some("1"));
    assert_eq!(checkpoint.remote_id, "old-id");
    server.join().unwrap();
}
