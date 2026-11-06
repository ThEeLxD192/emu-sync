//! Recovery of uploads whose outcome was lost when the process ended.

use emusync_data_drive::{DriveListing, DriveTransfer, TransferError};
use emusync_data_local::{
    ConfigManager, RecoveryCredentials, RecoveryRecord, RecoveryState, RecoveryStore,
    SyncHistoryStore, TrackedError, TrackedFileSync, UploadRequest, UploadSnapshot,
};
use emusync_desktop_app::{PersistedRecoveryStatus, reconcile_persisted_uploads};
use emusync_domain::{
    config::{AppConfig, CredentialStore, Credentials},
    sync::RemoteState,
};
use reqwest::{Client, Url};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    process::Command,
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};
use tokio_util::sync::CancellationToken;

const HASH: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn credentials(root: &Path) -> RecoveryCredentials<ConfigManager> {
    let store = Arc::new(ConfigManager::new(&root.join("config.json")).unwrap());
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
    RecoveryCredentials::new(store, snapshot, config)
}
fn request(stream: &mut TcpStream) -> (String, Vec<u8>) {
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
fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}
const OLD: &str = r#"{"id":"old","title":"save.dat","modifiedDate":"2026-01-01T00:00:00Z","etag":"\"v1\"","version":"1"}"#;
fn observed() -> RemoteState {
    RemoteState {
        id: "old".into(),
        modified: 1_700_000_000_000_000_000,
        sha256: None,
        version: Some("1".into()),
    }
}
struct Urls {
    v3: Url,
    v2: Url,
    upload: Url,
}
fn urls(root: &str) -> Urls {
    Urls {
        v3: Url::parse(&format!("{root}v3")).unwrap(),
        v2: Url::parse(&format!("{root}v2")).unwrap(),
        upload: Url::parse(&format!("{root}upload")).unwrap(),
    }
}
fn tracked(urls: &Urls, dir: &Path) -> TrackedFileSync<DriveTransfer> {
    let transfer = DriveTransfer::new(
        Client::new(),
        urls.v3.clone(),
        urls.v3.clone(),
        urls.v2.clone(),
        urls.upload.clone(),
        CancellationToken::new(),
        Arc::new(|_| {}),
    );
    let config = dir.join("config.json");
    TrackedFileSync::new(transfer, Arc::new(SyncHistoryStore::new(&config).unwrap()))
        .with_recovery(Arc::new(RecoveryStore::new(&config).unwrap()))
}
fn request_for<'a>(path: &'a str, remote: Option<&'a RemoteState>) -> UploadRequest<'a> {
    UploadRequest {
        token: "synthetic",
        local: path,
        remote,
        folder: "folder",
        remote_name: "save.dat",
    }
}
fn entries(dir: &Path) -> Vec<emusync_data_local::RecoveryEntry> {
    RecoveryStore::new(&dir.join("config.json"))
        .unwrap()
        .list()
        .unwrap()
}

/// Child process: starts an upload and waits for a response that never comes.
/// The parent kills it, so no destructor or cleanup ever runs.
#[test]
#[ignore = "re-executed as a child process by the killed-process test"]
fn child_uploads_and_hangs() {
    let (Some(dir), Some(root)) = (
        std::env::var_os("EMUSYNC_CHILD_DIR"),
        std::env::var_os("EMUSYNC_CHILD_ROOT"),
    ) else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let urls = urls(root.to_str().unwrap());
    let credentials = credentials(&dir);
    let save = dir.join("save.dat");
    let path = save.to_str().unwrap().to_owned();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let remote = observed();
            let _ = tracked(&urls, &dir)
                .upload_guarded(
                    "synthetic",
                    &path,
                    Some(&remote),
                    "folder",
                    "save.dat",
                    credentials,
                )
                .await;
        });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn killed_process_leaves_an_upload_that_a_new_process_reconciles_without_resending() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let (seen_tx, seen_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        assert!(request(&mut s).0.starts_with("GET /v2/old?"));
        respond(&mut s, "200 OK", OLD);
        let (mut put, _) = listener.accept().unwrap();
        let (headers, body) = request(&mut put);
        assert!(headers.starts_with("PUT /upload/old?"));
        assert_eq!(body, b"abc");
        seen_tx.send(()).unwrap();
        // The restarted process only reads: any further PUT would fail here.
        let (mut s, _) = listener.accept().unwrap();
        let (headers, _) = request(&mut s);
        assert!(headers.starts_with("GET /v3/old?"), "{headers}");
        let body = format!(
            r#"{{"id":"old","name":"save.dat","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","version":"2","sha256Checksum":"{HASH}"}}"#
        );
        respond(&mut s, "200 OK", &body);
        drop(put);
    });
    let dir = tempfile::tempdir().unwrap();
    let credentials = credentials(dir.path());
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "child_uploads_and_hangs",
            "--ignored",
            "--nocapture",
        ])
        .env("EMUSYNC_CHILD_DIR", dir.path())
        .env("EMUSYNC_CHILD_ROOT", &root)
        .spawn()
        .unwrap();
    seen_rx.recv_timeout(Duration::from_secs(20)).unwrap();
    child.kill().unwrap();
    child.wait().unwrap();

    // A new process sees the unresolved upload and its exact snapshot.
    let found = entries(dir.path());
    assert_eq!(found.len(), 1);
    let RecoveryState::Verified { record, snapshot } = &found[0].state else {
        panic!("unexpected {:?}", found[0].state);
    };
    assert_eq!(record.remote_id.as_deref(), Some("old"));
    assert_eq!(std::fs::read(snapshot).unwrap(), b"abc");
    let history = SyncHistoryStore::new(&dir.path().join("config.json")).unwrap();
    let key = history.key(&save, "folder", "save.dat").unwrap();
    assert!(history.get(&key).unwrap().is_none());

    // A new write for the same save waits for recovery and sends nothing.
    let urls = urls(&root);
    let path = save.to_str().unwrap().to_owned();
    let blocked = tracked(&urls, dir.path())
        .upload_guarded_classified(
            request_for(&path, Some(&observed())),
            credentials.clone(),
            |_: &TransferError| true,
        )
        .await;
    assert!(matches!(
        blocked,
        Err(TrackedError::Local(ref error)) if error.kind() == std::io::ErrorKind::ResourceBusy
    ));

    let tracked = tracked(&urls, dir.path());
    let listing = DriveListing::new(Client::new(), urls.v3.clone());
    let store = Arc::new(RecoveryStore::new(&dir.path().join("config.json")).unwrap());
    let results = reconcile_persisted_uploads(store, &listing, &tracked, "synthetic", credentials)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].1, PersistedRecoveryStatus::Recovered);
    assert!(entries(dir.path()).is_empty());
    assert_eq!(history.get(&key).unwrap().unwrap().remote_id, "old");
    assert_eq!(std::fs::read(&save).unwrap(), b"abc");
    server.join().unwrap();
}

fn stage(dir: &Path, connection: &str, remote_id: Option<&str>, digest_of: &[u8]) -> String {
    let source = dir.join("save.dat");
    std::fs::write(&source, digest_of).unwrap();
    let snapshot = UploadSnapshot::prepare(&source).unwrap();
    RecoveryStore::new(&dir.join("config.json"))
        .unwrap()
        .stage(
            &snapshot,
            RecoveryRecord {
                remote_id: remote_id.map(str::to_owned),
                local: source.to_str().unwrap().to_owned(),
                folder: "folder".into(),
                remote_name: "save.dat".into(),
                observed: None,
                digest: snapshot.digest().to_owned(),
                connection: connection.into(),
            },
        )
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entry_from_another_connection_is_neither_used_nor_removed() {
    let dir = tempfile::tempdir().unwrap();
    let credentials = credentials(dir.path());
    stage(dir.path(), "another connection", Some("old"), b"abc");
    // Nothing listens here: any request would fail the operation.
    let urls = urls("http://127.0.0.1:9/");
    let results = reconcile_persisted_uploads(
        Arc::new(RecoveryStore::new(&dir.path().join("config.json")).unwrap()),
        &DriveListing::new(Client::new(), urls.v3.clone()),
        &tracked(&urls, dir.path()),
        "synthetic",
        credentials,
    )
    .await
    .unwrap();
    assert_eq!(results[0].1, PersistedRecoveryStatus::ConnectionChanged);
    assert_eq!(entries(dir.path()).len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn created_upload_without_identity_reconciles_only_by_name_and_checksum() {
    for matching in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let credentials = credentials(dir.path());
        let probe = credentials.fingerprint();
        stage(dir.path(), &probe, None, b"abc");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let root = format!("http://{}/", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let (headers, _) = request(&mut s);
            assert!(headers.starts_with("GET /v3?"), "{headers}");
            assert!(headers.contains("save.dat"));
            let hash = if matching { HASH } else { "wrong" };
            let body = format!(
                r#"{{"files":[{{"id":"created","name":"save.dat","modifiedTime":"2026-01-01T00:00:00Z","sha256Checksum":"{hash}","version":"1"}}]}}"#
            );
            respond(&mut s, "200 OK", &body);
        });
        let urls = urls(&root);
        let results = reconcile_persisted_uploads(
            Arc::new(RecoveryStore::new(&dir.path().join("config.json")).unwrap()),
            &DriveListing::new(Client::new(), urls.v3.clone()),
            &tracked(&urls, dir.path()),
            "synthetic",
            credentials,
        )
        .await
        .unwrap();
        server.join().unwrap();
        if matching {
            assert_eq!(results[0].1, PersistedRecoveryStatus::Recovered);
            assert!(entries(dir.path()).is_empty());
        } else {
            assert_eq!(results[0].1, PersistedRecoveryStatus::RemoteUnconfirmed);
            assert_eq!(entries(dir.path()).len(), 1);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entries_follow_the_remote_outcome_of_each_upload() {
    // (PUT reply, expected entries left): success and a failed precondition prove
    // the outcome; a lost response does not.
    for (reply, left) in [
        (Some("200 OK"), 0),
        (Some("412 Precondition Failed"), 0),
        (None, 1),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let root = format!("http://{}/", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            request(&mut s);
            respond(&mut s, "200 OK", OLD);
            let (mut s, _) = listener.accept().unwrap();
            request(&mut s);
            match reply {
                Some("200 OK") => respond(
                    &mut s,
                    "200 OK",
                    r#"{"id":"old","title":"save.dat","modifiedDate":"2026-01-01T00:00:00Z","etag":"\"v2\"","version":"2"}"#,
                ),
                Some(status) => respond(&mut s, status, "{}"),
                None => drop(s),
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let credentials = credentials(dir.path());
        let save = dir.path().join("save.dat");
        std::fs::write(&save, b"abc").unwrap();
        let path = save.to_str().unwrap().to_owned();
        let result = tracked(&urls(&root), dir.path())
            .upload_guarded_classified(
                request_for(&path, Some(&observed())),
                credentials,
                TransferError::leaves_remote_unchanged,
            )
            .await;
        server.join().unwrap();
        assert_eq!(result.is_ok(), reply == Some("200 OK"), "{reply:?}");
        assert_eq!(entries(dir.path()).len(), left, "{reply:?}");
    }
}
