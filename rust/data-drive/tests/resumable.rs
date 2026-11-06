use emusync_data_drive::ResumableUpload;
use reqwest::{Client, Url};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
};
use tokio_util::sync::CancellationToken;
fn read_request(stream: &mut TcpStream) -> (String, Vec<u8>) {
    let mut bytes = Vec::new();
    let mut buf = [0; 4096];
    let end = loop {
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
        if let Some(i) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
    let length = headers
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .and_then(|s| s.parse::<usize>().ok())
        })
        .unwrap_or(0);
    while bytes.len() < end + length {
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
    }
    (headers, bytes[end..end + length].to_vec())
}
fn reply(stream: &mut TcpStream, status: &str, extra: &str, body: &str) {
    write!(stream,"HTTP/1.1 {status}\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
}
#[tokio::test]
async fn resumes_from_acknowledged_offset_and_keeps_exact_bytes() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let url = Url::parse(&format!("http://{address}/upload")).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut s);
        assert!(headers.starts_with("POST /upload?"));
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["name"], "save");
        assert_eq!(value["parents"][0], "folder");
        reply(&mut s, "200 OK", "Location: /session\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut s);
        assert!(headers.starts_with("PUT /session"));
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes 0-5/6")
        );
        assert_eq!(body, b"abcdef");
        reply(&mut s, "308 Resume Incomplete", "Range: bytes=0-2\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut s);
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes 3-5/6")
        );
        assert_eq!(body, b"def");
        reply(
            &mut s,
            "200 OK",
            "",
            r#"{"id":"created","name":"save","modifiedTime":"2026-01-01T00:00:00Z"}"#,
        );
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abcdef").unwrap();
    let progress = std::sync::Mutex::new(Vec::new());
    let result = ResumableUpload::new(Client::new(), url)
        .upload_with_progress(
            "synthetic",
            &path,
            "save",
            "folder",
            &CancellationToken::new(),
            &|event| progress.lock().unwrap().push(event),
        )
        .await
        .unwrap();
    assert_eq!(result.id, "created");
    let events = progress.into_inner().unwrap();
    assert_eq!(
        events
            .iter()
            .map(|event| (event.name.as_str(), event.transferred, event.total))
            .collect::<Vec<_>>(),
        [("save", 3, Some(6)), ("save", 6, Some(6))]
    );
    server.join().unwrap();
}

#[tokio::test]
async fn large_snapshot_uses_eight_mib_chunks_and_resumes_at_acknowledged_offset() {
    const CHUNK: usize = 8 * 1024 * 1024;
    let bytes: Vec<u8> = (0..CHUNK + 1024).map(|i| (i % 251) as u8).collect();
    let expected = bytes.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", "Location: /session\r\n", "");
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut stream);
        assert!(headers.to_ascii_lowercase().contains(&format!(
            "content-range: bytes 0-{}/{}",
            CHUNK - 1,
            expected.len()
        )));
        assert_eq!(body, expected[..CHUNK]);
        reply(
            &mut stream,
            "308 Resume Incomplete",
            &format!("Range: bytes=0-{}\r\n", CHUNK - 1),
            "",
        );
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut stream);
        assert!(headers.to_ascii_lowercase().contains(&format!(
            "content-range: bytes {}-{}/{}",
            CHUNK,
            expected.len() - 1,
            expected.len()
        )));
        assert_eq!(body, expected[CHUNK..]);
        reply(
            &mut stream,
            "200 OK",
            "",
            r#"{"id":"created","name":"save","modifiedTime":"2026-01-01T00:00:00Z"}"#,
        );
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, &bytes).unwrap();
    let progress = std::sync::Mutex::new(Vec::new());
    let result = ResumableUpload::new(
        Client::new(),
        Url::parse(&format!("http://{address}/upload")).unwrap(),
    )
    .upload_with_progress(
        "synthetic",
        &path,
        "save",
        "folder",
        &CancellationToken::new(),
        &|event| progress.lock().unwrap().push(event.transferred),
    )
    .await
    .unwrap();
    assert_eq!(result.id, "created");
    assert_eq!(
        progress.into_inner().unwrap(),
        [CHUNK as u64, bytes.len() as u64]
    );
    server.join().unwrap();
}

#[tokio::test]
async fn server_cannot_acknowledge_bytes_beyond_the_chunk_sent() {
    const CHUNK: usize = 8 * 1024 * 1024;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", "Location: /session\r\n", "");
        let (mut stream, _) = listener.accept().unwrap();
        let (_, body) = read_request(&mut stream);
        assert_eq!(body.len(), CHUNK);
        reply(
            &mut stream,
            "308 Resume Incomplete",
            &format!("Range: bytes=0-{CHUNK}\r\n"),
            "",
        );
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, vec![7; CHUNK + 1024]).unwrap();
    let error = ResumableUpload::new(
        Client::new(),
        Url::parse(&format!("http://{address}/upload")).unwrap(),
    )
    .upload(
        "synthetic",
        &path,
        "save",
        "folder",
        &CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, emusync_data_drive::UploadError::RecoveryFailed { cause, .. }
        if matches!(*cause, emusync_data_drive::UploadError::InvalidRange))
    );
    server.join().unwrap();
}

#[tokio::test]
async fn premature_completion_before_final_chunk_retains_session() {
    use emusync_data_drive::UploadError;
    const CHUNK: usize = 8 * 1024 * 1024;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", "Location: /session\r\n", "");
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut stream);
        assert_eq!(body.len(), CHUNK);
        assert!(headers.to_ascii_lowercase().contains(&format!(
            "content-range: bytes 0-{}/{}",
            CHUNK - 1,
            CHUNK + 1024
        )));
        reply(
            &mut stream,
            "200 OK",
            "",
            r#"{"id":"created","name":"save","modifiedTime":"2026-01-01T00:00:00Z"}"#,
        );
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, vec![7; CHUNK + 1024]).unwrap();
    let error = ResumableUpload::new(
        Client::new(),
        Url::parse(&format!("http://{address}/upload")).unwrap(),
    )
    .upload(
        "synthetic",
        &path,
        "save",
        "folder",
        &CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, UploadError::RecoveryFailed { cause, .. }
        if matches!(&*cause, UploadError::PrematureCompletion(file) if file.id == "created")));
    server.join().unwrap();
}

#[tokio::test]
async fn cross_origin_session_is_rejected_without_contacting_other_origin() {
    use emusync_data_drive::{DriveHttpPolicy, UploadError};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let trap = TcpListener::bind("127.0.0.1:0").unwrap();
    trap.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let trap_address = trap.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(
            &mut stream,
            "200 OK",
            &format!("Location: http://{trap_address}/session\r\n"),
            "",
        );
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abc").unwrap();
    let clients = DriveHttpPolicy::default().build().unwrap();
    let error = ResumableUpload::with_clients(
        clients.metadata,
        clients.upload,
        Url::parse(&format!("http://{address}/upload")).unwrap(),
    )
    .upload(
        "synthetic-token",
        &path,
        "save",
        "folder",
        &CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, UploadError::InvalidSession));
    server.join().unwrap();
    assert_eq!(
        trap.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn session_url_rejects_userinfo_and_fragment() {
    use emusync_data_drive::UploadError;
    for with_userinfo in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_request(&mut stream);
            let location = if with_userinfo {
                format!("http://user:password@{address}/session")
            } else {
                format!("http://{address}/session#fragment")
            };
            reply(
                &mut stream,
                "200 OK",
                &format!("Location: {location}\r\n"),
                "",
            );
            listener.set_nonblocking(true).unwrap();
            listener
        });
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snapshot");
        std::fs::write(&path, b"abc").unwrap();
        let error = ResumableUpload::new(
            Client::new(),
            Url::parse(&format!("http://{address}/upload")).unwrap(),
        )
        .upload(
            "synthetic-token",
            &path,
            "save",
            "folder",
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, UploadError::InvalidSession));
        assert_eq!(
            server.join().unwrap().accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}

#[tokio::test]
async fn session_redirect_is_not_followed_by_production_clients() {
    use emusync_data_drive::{DriveHttpPolicy, UploadError};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let trap = TcpListener::bind("127.0.0.1:0").unwrap();
    trap.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let trap_address = trap.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", "Location: /session\r\n", "");
        let (mut stream, _) = listener.accept().unwrap();
        let (_, body) = read_request(&mut stream);
        assert_eq!(body, b"abc");
        reply(
            &mut stream,
            "307 Temporary Redirect",
            &format!("Location: http://{trap_address}/other\r\n"),
            "",
        );
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abc").unwrap();
    let clients = DriveHttpPolicy::default().build().unwrap();
    let error = ResumableUpload::with_clients(
        clients.metadata,
        clients.upload,
        Url::parse(&format!("http://{address}/upload")).unwrap(),
    )
    .upload(
        "synthetic-token",
        &path,
        "save",
        "folder",
        &CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, UploadError::RecoveryFailed { cause, .. } if matches!(*cause, UploadError::Http(reqwest::StatusCode::TEMPORARY_REDIRECT)))
    );
    server.join().unwrap();
    assert_eq!(
        trap.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn repeated_zero_acknowledgements_stop_with_recoverable_session() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", "Location: /session\r\n", "");
        for _ in 0..4 {
            let (mut stream, _) = listener.accept().unwrap();
            let (headers, body) = read_request(&mut stream);
            assert!(
                headers
                    .to_ascii_lowercase()
                    .contains("content-range: bytes 0-2/3")
            );
            assert_eq!(body, b"abc");
            reply(&mut stream, "308 Resume Incomplete", "", "");
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abc").unwrap();
    let error = ResumableUpload::new(
        Client::new(),
        Url::parse(&format!("http://{address}/upload")).unwrap(),
    )
    .upload(
        "synthetic",
        &path,
        "save",
        "folder",
        &CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, emusync_data_drive::UploadError::RecoveryFailed { cause, .. }
        if matches!(*cause, emusync_data_drive::UploadError::NoProgress))
    );
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_during_success_metadata_retains_upload_session() {
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", "Location: /session\r\n", "");
        let (mut stream, _) = listener.accept().unwrap();
        let (_, body) = read_request(&mut stream);
        assert_eq!(body, b"abc");
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{{"
        )
        .unwrap();
        stream.flush().unwrap();
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abc").unwrap();
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    let task = tokio::spawn(async move {
        ResumableUpload::new(Client::new(), url)
            .upload("synthetic", &path, "save", "folder", &signal)
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        result,
        Err(emusync_data_drive::UploadError::OutcomeUnknown(_))
    ));
    release_tx.send(()).unwrap();
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_during_stalled_status_query_retains_upload_session() {
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", "Location: /session\r\n", "");
        let (mut stream, _) = listener.accept().unwrap();
        assert_eq!(read_request(&mut stream).1, b"abc");
        drop(stream);
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut stream);
        assert!(body.is_empty());
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes */3")
        );
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abc").unwrap();
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    let task = tokio::spawn(async move {
        ResumableUpload::new(Client::new(), url)
            .upload("synthetic", &path, "save", "folder", &signal)
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        result,
        Err(emusync_data_drive::UploadError::OutcomeUnknown(_))
    ));
    release_tx.send(()).unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn stalled_success_metadata_queries_session_without_resending_snapshot() {
    use std::time::Duration;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", "Location: /session\r\n", "");
        let (mut stream, _) = listener.accept().unwrap();
        assert_eq!(read_request(&mut stream).1, b"abc");
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{{"
        )
        .unwrap();
        stream.flush().unwrap();
        let (mut query, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut query);
        assert!(body.is_empty());
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes */3")
        );
        reply(
            &mut query,
            "200 OK",
            "",
            r#"{"id":"created","name":"save","modifiedTime":"2026-01-01T00:00:00Z"}"#,
        );
        drop(stream);
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abc").unwrap();
    let result = ResumableUpload::new(Client::new(), url)
        .with_response_inactivity(Duration::from_millis(150))
        .upload(
            "synthetic",
            &path,
            "save",
            "folder",
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result.id, "created");
    server.join().unwrap();
}

#[tokio::test]
async fn oversized_success_metadata_queries_status_without_resending_save() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", "Location: /session\r\n", "");
        let (mut stream, _) = listener.accept().unwrap();
        assert_eq!(read_request(&mut stream).1, b"abc");
        let body = vec![b'x'; 1024 * 1024 + 1];
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        let _ = stream.write_all(&body);
        let (mut query, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut query);
        assert!(body.is_empty());
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes */3")
        );
        reply(
            &mut query,
            "200 OK",
            "",
            r#"{"id":"created","name":"save","modifiedTime":"2026-01-01T00:00:00Z"}"#,
        );
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abc").unwrap();
    let result = ResumableUpload::new(Client::new(), url)
        .upload(
            "synthetic",
            &path,
            "save",
            "folder",
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result.id, "created");
    server.join().unwrap();
}

#[tokio::test]
async fn lost_upload_response_queries_status_before_resuming() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let url = Url::parse(&format!("http://{address}/upload")).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", "Location: /session\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        let (_, body) = read_request(&mut s);
        assert_eq!(body, b"abcdef");
        drop(s);
        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut s);
        assert!(body.is_empty());
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes */6")
        );
        reply(&mut s, "308 Resume Incomplete", "Range: bytes=0-2\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut s);
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes 3-5/6")
        );
        assert_eq!(body, b"def");
        reply(
            &mut s,
            "200 OK",
            "",
            r#"{"id":"created","name":"save","modifiedTime":"2026-01-01T00:00:00Z"}"#,
        );
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abcdef").unwrap();
    let result = ResumableUpload::new(Client::new(), url)
        .upload(
            "synthetic",
            &path,
            "save",
            "folder",
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result.id, "created");
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interrupted_request_reports_uncertain_remote_outcome() {
    use emusync_data_drive::{ReconcileStatus, UploadError};
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let url = Url::parse(&format!("http://{address}/upload")).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", "Location: /session\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        let (_, body) = read_request(&mut s);
        assert_eq!(body, b"abcdef");
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(s);
        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut s);
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes */6")
        );
        assert!(body.is_empty());
        reply(
            &mut s,
            "200 OK",
            "",
            r#"{"id":"created","name":"save","modifiedTime":"2026-01-01T00:00:00Z"}"#,
        );
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abcdef").unwrap();
    let cancel = CancellationToken::new();
    let child = cancel.clone();
    let reconcile_url = url.clone();
    let task = tokio::spawn(async move {
        ResumableUpload::new(Client::new(), url)
            .upload("synthetic", &path, "save", "folder", &child)
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    let session = match result {
        Err(UploadError::OutcomeUnknown(session)) => session,
        other => panic!("unexpected result: {other:?}"),
    };
    assert!(!format!("{session:?}").contains("session"));
    release_tx.send(()).unwrap();
    let status = ResumableUpload::new(Client::new(), reconcile_url)
        .reconcile("synthetic", &session)
        .await
        .unwrap();
    assert!(matches!(status, ReconcileStatus::Complete(file) if file.id == "created"));
    server.join().unwrap();
}

#[tokio::test]
async fn truncated_success_metadata_is_reconciled_before_returning() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let url = Url::parse(&format!("http://{address}/upload")).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", "Location: /session\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{{\"id\":"
        )
        .unwrap();
        drop(s);
        let (mut s, _) = listener.accept().unwrap();
        let (headers, _) = read_request(&mut s);
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes */6")
        );
        reply(
            &mut s,
            "200 OK",
            "",
            r#"{"id":"created","name":"save","modifiedTime":"2026-01-01T00:00:00Z"}"#,
        );
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abcdef").unwrap();
    let file = ResumableUpload::new(Client::new(), url)
        .upload(
            "synthetic",
            &path,
            "save",
            "folder",
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(file.id, "created");
    server.join().unwrap();
}

#[tokio::test]
async fn expired_session_restarts_once_and_uploads_empty_snapshot() {
    use emusync_data_drive::DriveHttpPolicy;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let url = Url::parse(&format!("http://{address}/upload")).unwrap();
    let server = thread::spawn(move || {
        for attempt in 0..2 {
            let (mut s, _) = listener.accept().unwrap();
            let (headers, _) = read_request(&mut s);
            assert!(headers.starts_with("POST /upload?"));
            reply(&mut s, "200 OK", "Location: /session\r\n", "");
            let (mut s, _) = listener.accept().unwrap();
            let (headers, body) = read_request(&mut s);
            assert!(headers.starts_with("PUT /session"));
            assert!(body.is_empty());
            if attempt == 0 {
                reply(&mut s, "404 Not Found", "", "");
            } else {
                reply(
                    &mut s,
                    "200 OK",
                    "",
                    r#"{"id":"empty","name":"save","modifiedTime":"2026-01-01T00:00:00Z"}"#,
                );
            }
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"").unwrap();
    let clients = DriveHttpPolicy::default().build().unwrap();
    let result = ResumableUpload::with_transfer_clients(
        clients.metadata,
        clients.upload,
        clients.empty_upload,
        url,
    )
    .upload(
        "synthetic",
        &path,
        "save",
        "folder",
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(result.id, "empty");
    server.join().unwrap();
}

#[tokio::test]
async fn invalid_server_range_is_rejected_without_resending_data() {
    use emusync_data_drive::UploadError;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let url = Url::parse(&format!("http://{address}/upload")).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", "Location: /session\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(
            &mut s,
            "308 Resume Incomplete",
            "Range: bytes=0-600\r\n",
            "",
        );
        listener.set_nonblocking(true).unwrap();
        listener
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abcdef").unwrap();
    let result = ResumableUpload::new(Client::new(), url)
        .upload(
            "synthetic",
            &path,
            "save",
            "folder",
            &CancellationToken::new(),
        )
        .await;
    assert!(
        matches!(result, Err(UploadError::RecoveryFailed { cause, .. }) if matches!(*cause, UploadError::InvalidRange))
    );
    let listener = server.join().unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn expired_status_after_lost_response_does_not_create_a_duplicate() {
    use emusync_data_drive::UploadError;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", "Location: /session\r\n", "");
        let (mut stream, _) = listener.accept().unwrap();
        let (_, body) = read_request(&mut stream);
        assert_eq!(body, b"abcdef");
        drop(stream);
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, _) = read_request(&mut stream);
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes */6")
        );
        reply(&mut stream, "404 Not Found", "", "");
        listener.set_nonblocking(true).unwrap();
        listener
    });
    let dir = tempfile::tempdir().unwrap();
    let snapshot = dir.path().join("save");
    std::fs::write(&snapshot, b"abcdef").unwrap();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        ResumableUpload::new(Client::new(), url).upload(
            "synthetic",
            &snapshot,
            "save",
            "folder",
            &CancellationToken::new(),
        ),
    )
    .await
    .unwrap();
    assert!(matches!(result, Err(UploadError::OutcomeUnknown(_))));
    let listener = server.join().unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn unexpected_upload_identity_retains_session_until_verified() {
    use emusync_data_drive::{ReconcileStatus, UploadError};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", "Location: /session\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(
            &mut s,
            "200 OK",
            "",
            r#"{"id":"  ","name":"save","modifiedTime":"2026-01-01T00:00:00Z"}"#,
        );
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(
            &mut s,
            "200 OK",
            "",
            r#"{"id":"wrong","name":"other","modifiedTime":"2026-01-01T00:00:00Z"}"#,
        );
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(
            &mut s,
            "200 OK",
            "",
            r#"{"id":"right","name":"save","modifiedTime":"2026-01-01T00:00:00Z"}"#,
        );
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"data").unwrap();
    let client = ResumableUpload::new(Client::new(), url);
    let first = client
        .upload(
            "synthetic",
            &path,
            "save",
            "folder",
            &CancellationToken::new(),
        )
        .await;
    let session = match first {
        Err(UploadError::OutcomeUnknown(session)) => session,
        other => panic!("unexpected {other:?}"),
    };
    assert!(matches!(
        client.reconcile("synthetic", &session).await,
        Err(UploadError::OutcomeUnknown(_))
    ));
    assert!(
        matches!(client.reconcile("synthetic",&session).await,Ok(ReconcileStatus::Complete(file)) if file.id=="right")
    );
    server.join().unwrap();
}

#[tokio::test]
async fn status_failure_retains_session_for_later_reconciliation() {
    use emusync_data_drive::{ReconcileStatus, UploadError};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", "Location: /session\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        drop(s);
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "403 Forbidden", "", "");
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(
            &mut s,
            "200 OK",
            "",
            r#"{"id":"created","name":"save","modifiedTime":"2026-01-01T00:00:00Z"}"#,
        );
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"data").unwrap();
    let client = ResumableUpload::new(Client::new(), url);
    let result = client
        .upload(
            "synthetic",
            &path,
            "save",
            "folder",
            &CancellationToken::new(),
        )
        .await;
    let session = match result {
        Err(UploadError::RecoveryFailed { session, cause })
            if matches!(*cause, UploadError::Http(reqwest::StatusCode::FORBIDDEN)) =>
        {
            session
        }
        other => panic!("unexpected {other:?}"),
    };
    assert!(
        matches!(client.reconcile("synthetic",&session).await, Ok(ReconcileStatus::Complete(file)) if file.id=="created")
    );
    server.join().unwrap();
}

#[tokio::test]
async fn chunk_uses_transfer_timeout_instead_of_short_control_timeout() {
    use emusync_data_drive::DriveHttpPolicy;
    use std::time::Duration;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", "Location: /session\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        let (_, body) = read_request(&mut s);
        assert_eq!(body, b"abc");
        thread::sleep(Duration::from_millis(300));
        reply(
            &mut s,
            "200 OK",
            "",
            r#"{"id":"created","name":"save","modifiedTime":"2026-01-01T00:00:00Z"}"#,
        );
    });
    let clients = DriveHttpPolicy {
        connect: Duration::from_secs(2),
        metadata: Duration::from_millis(150),
        inactivity: Duration::from_millis(700),
    }
    .build()
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abc").unwrap();
    let file = ResumableUpload::with_clients(clients.metadata, clients.upload, url)
        .upload(
            "synthetic",
            &path,
            "save",
            "folder",
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(file.id, "created");
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn expired_reconciliation_explicitly_preserves_uncertainty() {
    use emusync_data_drive::{ReconcileStatus, UploadError};
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", "Location: /session\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(s);
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "404 Not Found", "", "");
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abc").unwrap();
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();
    let task_url = url.clone();
    let task = tokio::spawn(async move {
        ResumableUpload::new(Client::new(), task_url)
            .upload("synthetic", &path, "save", "folder", &task_cancel)
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    cancel.cancel();
    let session = match task.await.unwrap() {
        Err(UploadError::OutcomeUnknown(session)) => session,
        other => panic!("unexpected {other:?}"),
    };
    release_tx.send(()).unwrap();
    assert!(matches!(
        ResumableUpload::new(Client::new(), url)
            .reconcile("synthetic", &session)
            .await,
        Ok(ReconcileStatus::ExpiredOutcomeUnknown)
    ));
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stalled_request_body_can_be_cancelled_with_recoverable_session() {
    use emusync_data_drive::UploadError;
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", "Location: /session\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        let mut headers = Vec::new();
        let mut one = [0; 1];
        while !headers.ends_with(b"\r\n\r\n") {
            assert_eq!(s.read(&mut one).unwrap(), 1);
            headers.push(one[0]);
            assert!(headers.len() < 8192);
        }
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, vec![7; 8 * 1024 * 1024]).unwrap();
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    let mut task = tokio::spawn(async move {
        ResumableUpload::new(Client::new(), url)
            .upload("synthetic", &path, "save", "folder", &signal)
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(150), &mut task)
            .await
            .is_err()
    );
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(result, Err(UploadError::OutcomeUnknown(_))));
    release_tx.send(()).unwrap();
    server.join().unwrap();
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn linux_tcp_timeout_queries_stalled_upload_and_retains_unknown_session() {
    use emusync_data_drive::{DriveHttpPolicy, UploadError};
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", "Location: /session\r\n", "");
        let (mut stream, _) = listener.accept().unwrap();
        socket2::SockRef::from(&stream)
            .set_recv_buffer_size(4096)
            .unwrap();
        let mut headers = Vec::new();
        let mut byte = [0; 1];
        while !headers.ends_with(b"\r\n\r\n") {
            assert_eq!(stream.read(&mut byte).unwrap(), 1);
            headers.push(byte[0]);
        }
        seen_tx.send(()).unwrap();
        let holder = thread::spawn(move || {
            release_rx.recv_timeout(Duration::from_secs(15)).unwrap();
            drop(stream);
        });
        let (mut query, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut query);
        assert!(body.is_empty());
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes */8388608")
        );
        reply(&mut query, "404 Not Found", "", "");
        holder.join().unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, vec![7; 8 * 1024 * 1024]).unwrap();
    let clients = DriveHttpPolicy {
        connect: Duration::from_secs(2),
        metadata: Duration::from_secs(2),
        inactivity: Duration::from_millis(500),
    }
    .build()
    .unwrap();
    let task = tokio::spawn(async move {
        ResumableUpload::with_clients(clients.metadata, clients.upload, url)
            .upload(
                "synthetic",
                &path,
                "save",
                "folder",
                &CancellationToken::new(),
            )
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(result, Err(UploadError::OutcomeUnknown(_))));
    release_tx.send(()).unwrap();
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn empty_resumable_upload_times_out_and_retains_unknown_session() {
    use emusync_data_drive::{DriveHttpPolicy, UploadError};
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", "Location: /session\r\n", "");
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut stream);
        assert!(headers.to_ascii_lowercase().contains("content-length: 0"));
        assert!(body.is_empty());
        seen_tx.send(()).unwrap();
        let holder = thread::spawn(move || {
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            drop(stream);
        });
        let (mut query, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut query);
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes */0")
        );
        assert!(body.is_empty());
        reply(&mut query, "404 Not Found", "", "");
        holder.join().unwrap();
        listener.set_nonblocking(true).unwrap();
        listener
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty");
    std::fs::write(&path, []).unwrap();
    let clients = DriveHttpPolicy {
        connect: Duration::from_secs(2),
        metadata: Duration::from_secs(2),
        inactivity: Duration::from_millis(200),
    }
    .build()
    .unwrap();
    let task = tokio::spawn(async move {
        ResumableUpload::with_transfer_clients(
            clients.metadata,
            clients.upload,
            clients.empty_upload,
            url,
        )
        .upload(
            "synthetic",
            &path,
            "save",
            "folder",
            &CancellationToken::new(),
        )
        .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap(),
        Err(UploadError::OutcomeUnknown(_))
    ));
    release_tx.send(()).unwrap();
    assert_eq!(
        server.join().unwrap().accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nonempty_resumable_upload_times_out_after_body_and_retains_unknown_session() {
    use emusync_data_drive::{DriveHttpPolicy, UploadError};
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", "Location: /session\r\n", "");
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut stream);
        assert!(headers.to_ascii_lowercase().contains("content-length: 3"));
        assert_eq!(body, b"abc");
        seen_tx.send(()).unwrap();
        let holder = thread::spawn(move || {
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            drop(stream);
        });
        let (mut query, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut query);
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes */3")
        );
        assert!(body.is_empty());
        reply(&mut query, "404 Not Found", "", "");
        holder.join().unwrap();
        listener.set_nonblocking(true).unwrap();
        listener
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nonempty");
    std::fs::write(&path, b"abc").unwrap();
    let clients = DriveHttpPolicy {
        connect: Duration::from_secs(2),
        metadata: Duration::from_secs(2),
        inactivity: Duration::from_millis(200),
    }
    .build()
    .unwrap();
    let task = tokio::spawn(async move {
        ResumableUpload::with_transfer_clients(
            clients.metadata,
            clients.upload.clone(),
            clients.upload,
            url,
        )
        .with_response_inactivity(Duration::from_millis(200))
        .upload(
            "synthetic",
            &path,
            "save",
            "folder",
            &CancellationToken::new(),
        )
        .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap(),
        Err(UploadError::OutcomeUnknown(_))
    ));
    release_tx.send(()).unwrap();
    assert_eq!(
        server.join().unwrap().accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slow_active_upload_can_exceed_body_inactivity_interval() {
    use emusync_data_drive::DriveHttpPolicy;
    use std::time::{Duration, Instant};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", "Location: /session\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        let mut headers = Vec::new();
        let mut one = [0; 1];
        while !headers.ends_with(b"\r\n\r\n") {
            assert_eq!(s.read(&mut one).unwrap(), 1);
            headers.push(one[0]);
            assert!(headers.len() < 8192);
        }
        let headers = String::from_utf8(headers).unwrap();
        let length = headers
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length: ")
                    .and_then(|value| value.parse::<usize>().ok())
            })
            .unwrap();
        let mut remaining = length;
        let mut buffer = [0; 16384];
        let start = Instant::now();
        while remaining > 0 {
            let limit = remaining.min(buffer.len());
            let n = s.read(&mut buffer[..limit]).unwrap();
            assert!(n > 0);
            remaining -= n;
            thread::sleep(Duration::from_millis(5));
        }
        assert!(start.elapsed() > Duration::from_millis(500));
        reply(
            &mut s,
            "200 OK",
            "",
            r#"{"id":"created","name":"save","modifiedTime":"2026-01-01T00:00:00Z"}"#,
        );
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, vec![7; 4 * 1024 * 1024]).unwrap();
    let clients = DriveHttpPolicy {
        connect: Duration::from_secs(2),
        metadata: Duration::from_secs(2),
        inactivity: Duration::from_millis(500),
    }
    .build()
    .unwrap();
    let result = ResumableUpload::with_clients(clients.metadata, clients.upload, url)
        .upload(
            "synthetic",
            &path,
            "save",
            "folder",
            &CancellationToken::new(),
        )
        .await;
    assert_eq!(result.unwrap().id, "created");
    server.join().unwrap();
}

#[tokio::test]
async fn all_bytes_acknowledged_queries_completion_without_resending() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", "Location: /session\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        let (_, body) = read_request(&mut s);
        assert_eq!(body, b"abc");
        reply(&mut s, "308 Resume Incomplete", "Range: bytes=0-2\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut s);
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes */3")
        );
        assert!(body.is_empty());
        reply(
            &mut s,
            "200 OK",
            "",
            r#"{"id":"created","name":"save","modifiedTime":"2026-01-01T00:00:00Z"}"#,
        );
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abc").unwrap();
    let result = ResumableUpload::new(Client::new(), url)
        .upload(
            "synthetic",
            &path,
            "save",
            "folder",
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result.id, "created");
    server.join().unwrap();
}

#[tokio::test]
async fn all_bytes_acknowledged_without_completion_retains_uncertain_session() {
    use emusync_data_drive::UploadError;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", "Location: /session\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "308 Resume Incomplete", "Range: bytes=0-2\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut s);
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes */3")
        );
        assert!(body.is_empty());
        reply(&mut s, "308 Resume Incomplete", "Range: bytes=0-2\r\n", "");
        listener.set_nonblocking(true).unwrap();
        listener
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abc").unwrap();
    let result = ResumableUpload::new(Client::new(), url)
        .upload(
            "synthetic",
            &path,
            "save",
            "folder",
            &CancellationToken::new(),
        )
        .await;
    assert!(matches!(result, Err(UploadError::OutcomeUnknown(_))));
    let listener = server.join().unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
