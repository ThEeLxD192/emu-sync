use emusync_data_drive::{DownloadError, DownloadOptions, DriveDownload};
use reqwest::{Client, Url};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
    time::Duration,
};
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_of_stalled_download_preserves_destination_and_cleans_stage() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        let n = stream.read(&mut request).unwrap();
        assert!(String::from_utf8_lossy(&request[..n]).starts_with("GET /files/save?alt=media"));
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\npartial"
        )
        .unwrap();
        stream.flush().unwrap();
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("save");
    std::fs::write(&destination, b"original").unwrap();
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();
    let path = destination.clone();
    let task = tokio::spawn(async move {
        DriveDownload::new(Client::new(), url)
            .download("synthetic", "save", &path, &task_cancel)
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let staged = std::fs::read_dir(dir.path())
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".emusync-download-")
                        // Not `entry.metadata()`: on Windows the directory entry keeps a
                        // stale size while the staging file is open for writing.
                        && std::fs::metadata(entry.path()).is_ok_and(|m| m.len() == 7)
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
    assert!(matches!(result, Err(DownloadError::Cancelled)));
    assert_eq!(std::fs::read(&destination).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    release_tx.send(()).unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn completed_download_replaces_destination() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        assert!(stream.read(&mut request).unwrap() > 0);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nnew"
        )
        .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("save");
    std::fs::write(&destination, b"old").unwrap();
    let progress = std::sync::Mutex::new(Vec::new());
    DriveDownload::new(Client::new(), url)
        .download_with(
            "synthetic",
            "id",
            &destination,
            &CancellationToken::new(),
            DownloadOptions {
                modified: Some(1_700_000_000_000_000_000),
                progress_name: "save",
                on_progress: &|p| progress.lock().unwrap().push(p),
            },
        )
        .await
        .unwrap();
    let events = progress.into_inner().unwrap();
    let last = events.last().unwrap();
    assert_eq!(
        (last.name.as_str(), last.transferred, last.total),
        ("save", 3, Some(3))
    );
    let modified =
        filetime::FileTime::from_last_modification_time(&std::fs::metadata(&destination).unwrap());
    assert_eq!(modified.unix_seconds(), 1_700_000_000);
    assert_eq!(std::fs::read(&destination).unwrap(), b"new");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    server.join().unwrap();
}

#[tokio::test]
async fn truncated_response_preserves_destination_and_cleans_stage() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        assert!(stream.read(&mut request).unwrap() > 0);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\ntruncated"
        )
        .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("save");
    std::fs::write(&destination, b"original").unwrap();
    let result = DriveDownload::new(Client::new(), url)
        .download("synthetic", "id", &destination, &CancellationToken::new())
        .await;
    assert!(matches!(result, Err(DownloadError::Transport(_))));
    assert_eq!(std::fs::read(&destination).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    server.join().unwrap();
}

#[tokio::test]
async fn http_error_response_preserves_destination_and_reports_error() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        assert!(stream.read(&mut request).unwrap() > 0);
        write!(
            stream,
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("save");
    std::fs::write(&destination, b"original").unwrap();
    let result = DriveDownload::new(Client::new(), url)
        .download("synthetic", "id", &destination, &CancellationToken::new())
        .await;
    assert!(matches!(
        result,
        Err(DownloadError::Http(reqwest::StatusCode::NOT_FOUND))
    ));
    assert_eq!(std::fs::read(&destination).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    server.join().unwrap();
}

#[tokio::test]
async fn invalid_download_file_ids_fail_without_network_or_touching_destination() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let dest_dir = dir.path().join("sub");
    let destination = dest_dir.join("save.dat");

    let invalid_ids = ["", "   ", ".", "..", "sub/id", "sub\\id", "id\0null"];
    let client = DriveDownload::new(Client::new(), url);
    let cancel = CancellationToken::new();

    for id in invalid_ids {
        let result = client
            .download("synthetic", id, &destination, &cancel)
            .await;
        assert!(matches!(result, Err(DownloadError::InvalidEndpoint)));
        assert!(!destination.exists());
        assert!(!dest_dir.exists());
    }

    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn inactivity_timeout_and_pre_epoch_timestamp_preserve_stage_hygiene() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let (release_tx, release_rx) = mpsc::channel();

    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut buf = [0; 1024];
        let _ = s.read(&mut buf).unwrap();
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nabc"
        )
        .unwrap();
        s.flush().unwrap();
        let _ = release_rx.recv();

        let (mut s, _) = listener.accept().unwrap();
        let _ = s.read(&mut buf).unwrap();
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\nnew-content"
        )
        .unwrap();
        s.flush().unwrap();
    });

    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("save.dat");
    std::fs::write(&destination, b"original").unwrap();

    let client = Client::builder()
        .read_timeout(Duration::from_millis(150))
        .build()
        .unwrap();
    let downloader = DriveDownload::new(client, url);

    let res = downloader
        .download(
            "synthetic",
            "file-id",
            &destination,
            &CancellationToken::new(),
        )
        .await;
    assert!(matches!(res, Err(DownloadError::Transport(ref e)) if e.is_timeout()));
    assert_eq!(std::fs::read(&destination).unwrap(), b"original");
    let has_parts = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .any(|e| {
            e.file_name()
                .to_string_lossy()
                .contains(".emusync-download-")
        });
    assert!(!has_parts);

    let _ = release_tx.send(());

    downloader
        .download_with(
            "synthetic",
            "file-id",
            &destination,
            &CancellationToken::new(),
            DownloadOptions {
                modified: Some(-1_500_000_000),
                progress_name: "save",
                on_progress: &|_| {},
            },
        )
        .await
        .unwrap();

    assert_eq!(std::fs::read(&destination).unwrap(), b"new-content");
    let has_parts = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .any(|e| {
            e.file_name()
                .to_string_lossy()
                .contains(".emusync-download-")
        });
    assert!(!has_parts);

    let mtime =
        filetime::FileTime::from_last_modification_time(&std::fs::metadata(&destination).unwrap());
    assert_eq!(mtime.unix_seconds(), -2);
    assert_eq!(mtime.nanoseconds(), 500_000_000);

    server.join().unwrap();
}
