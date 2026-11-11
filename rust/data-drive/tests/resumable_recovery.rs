mod common;

use common::{read_request, reply_extra as reply};
use emusync_data_drive::{ReconcileStatus, ResumableUpload, UploadError};
use reqwest::{Client, Url};
use std::{net::TcpListener, sync::mpsc, thread, time::Duration};
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_during_success_metadata_retains_upload_session() {
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
        use std::io::Write;
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
    assert!(matches!(result, Err(UploadError::OutcomeUnknown(_))));
    release_tx.send(()).unwrap();
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_during_stalled_status_query_retains_upload_session() {
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
    assert!(matches!(result, Err(UploadError::OutcomeUnknown(_))));
    release_tx.send(()).unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn stalled_success_metadata_queries_session_without_resending_snapshot() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", "Location: /session\r\n", "");
        let (mut stream, _) = listener.accept().unwrap();
        assert_eq!(read_request(&mut stream).1, b"abc");
        use std::io::Write;
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
        use std::io::Write;
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
        use std::io::Write;
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
async fn expired_status_after_lost_response_does_not_create_a_duplicate() {
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
        use std::io::Read;
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
    use emusync_data_drive::DriveHttpPolicy;
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
        use std::io::Read;
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
    use emusync_data_drive::DriveHttpPolicy;
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
    use emusync_data_drive::DriveHttpPolicy;
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
    use std::time::Instant;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", "Location: /session\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        let mut headers = Vec::new();
        let mut one = [0; 1];
        use std::io::Read;
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_during_no_progress_backoff_returns_unknown_outcome() {
    use std::io::Read;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let (done_tx, mut done_rx) = tokio::sync::mpsc::channel::<()>(1);

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", "Location: /session\r\n", "");

        let (mut stream, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut stream);
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes 0-5/6")
        );
        assert_eq!(body, b"abcdef");
        reply(
            &mut stream,
            "308 Resume Incomplete",
            "Range: bytes=0-2\r\n",
            "",
        );

        let (mut stream, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut stream);
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes 3-5/6")
        );
        assert_eq!(body, b"def");
        reply(
            &mut stream,
            "308 Resume Incomplete",
            "Range: bytes=0-2\r\n",
            "",
        );
        stream.shutdown(std::net::Shutdown::Write).unwrap();
        let mut eof = [0; 1];
        assert_eq!(stream.read(&mut eof).unwrap(), 0);
        listener.set_nonblocking(true).unwrap();
        done_tx.blocking_send(()).unwrap();
        listener
    });

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abcdef").unwrap();

    let cancel = CancellationToken::new();
    let client = ResumableUpload::new(Client::new(), url);
    let upload_future = client.upload("synthetic", &path, "save", "folder", &cancel);
    tokio::pin!(upload_future);

    while done_rx.try_recv().is_err() {
        assert!(
            futures_util::poll!(&mut upload_future).is_pending(),
            "upload future should not complete before server responds"
        );
        tokio::task::yield_now().await;
    }

    tokio::task::yield_now().await;
    tokio::task::yield_now().await;
    assert!(futures_util::poll!(&mut upload_future).is_pending());

    cancel.cancel();
    let poll = futures_util::poll!(&mut upload_future);
    assert!(matches!(
        poll,
        std::task::Poll::Ready(Err(UploadError::OutcomeUnknown(_)))
    ));

    let listener = server.join().unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconcile_reports_incomplete_offset_and_exhausts_retries_on_repeated_server_errors() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let url = Url::parse(&format!("http://{address}/upload")).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();

    let s1_listener = listener.try_clone().unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = s1_listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", "Location: /session\r\n", "");
        let (mut s, _) = s1_listener.accept().unwrap();
        let (_, body) = read_request(&mut s);
        assert_eq!(body, b"abcdef");
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(s);
    });

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abcdef").unwrap();

    let cancel = CancellationToken::new();
    let child = cancel.clone();
    let task_url = url.clone();
    let task = tokio::spawn(async move {
        ResumableUpload::new(Client::new(), task_url)
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
        Err(UploadError::OutcomeUnknown(session)) => *session,
        other => panic!("unexpected result: {other:?}"),
    };
    release_tx.send(()).unwrap();
    server.join().unwrap();

    // Inyección adversarial 1: reconcile responde 308 Incomplete con Range: bytes=0-3
    {
        let s2_listener = listener.try_clone().unwrap();
        let server = thread::spawn(move || {
            let (mut s, _) = s2_listener.accept().unwrap();
            let (headers, body) = read_request(&mut s);
            assert!(
                headers
                    .to_ascii_lowercase()
                    .contains("content-range: bytes */6")
            );
            assert!(body.is_empty());
            reply(&mut s, "308 Resume Incomplete", "Range: bytes=0-3\r\n", "");
        });

        let client = ResumableUpload::new(Client::new(), url.clone());
        let status = client.reconcile("synthetic", &session).await.unwrap();
        assert!(matches!(
            status,
            ReconcileStatus::Incomplete { received: 4 }
        ));
        server.join().unwrap();
    }

    // Inyección adversarial 2: reconcile recibe 3 veces consecutivas 503 Service Unavailable
    {
        let server = thread::spawn(move || {
            for _ in 0..3 {
                let (mut s, _) = listener.accept().unwrap();
                let (headers, body) = read_request(&mut s);
                assert!(
                    headers
                        .to_ascii_lowercase()
                        .contains("content-range: bytes */6")
                );
                assert!(body.is_empty());
                reply(&mut s, "503 Service Unavailable", "", "{}");
            }
            listener.set_nonblocking(true).unwrap();
            listener
        });

        let client = ResumableUpload::new(Client::new(), url);
        let result = client.reconcile("synthetic", &session).await;
        assert!(matches!(result, Err(UploadError::OutcomeUnknown(_))));
        let listener = server.join().unwrap();
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_after_session_start_before_first_chunk_put_returns_cancelled_not_unknown() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, _) = read_request(&mut stream);
        assert!(headers.starts_with("POST /upload?"));
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        reply(&mut stream, "200 OK", "Location: /session\r\n", "");
        listener.set_nonblocking(true).unwrap();
        listener
    });

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abc").unwrap();

    let cancel = CancellationToken::new();
    let cancel_task = cancel.clone();
    let task = tokio::spawn(async move {
        ResumableUpload::new(Client::new(), url)
            .upload("synthetic", &path, "save", "folder", &cancel_task)
            .await
    });

    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    cancel.cancel();
    release_tx.send(()).unwrap();

    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();

    assert!(matches!(result, Err(UploadError::Cancelled)));
    assert!(!matches!(result, Err(UploadError::OutcomeUnknown(_))));

    let listener = server.join().unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
