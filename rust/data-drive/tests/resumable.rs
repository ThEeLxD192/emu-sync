mod common;

use common::{read_request, reply_extra as reply};
use emusync_data_drive::{ResumableUpload, UploadError};
use reqwest::{Client, StatusCode, Url};
use std::{net::TcpListener, thread};
use tokio_util::sync::CancellationToken;

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
async fn large_snapshot_uses_eight_mib_chunks_and_resumes_at_acknowledged_offset() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let url = Url::parse(&format!("http://{address}/upload")).unwrap();
    let total = 8 * 1024 * 1024 + 10;
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", "Location: /session\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut s);
        assert!(headers.to_ascii_lowercase().contains(&format!(
            "content-range: bytes 0-{}/{total}",
            8 * 1024 * 1024 - 1
        )));
        assert_eq!(body.len(), 8 * 1024 * 1024);
        assert!(body.iter().all(|b| *b == 7));
        reply(
            &mut s,
            "308 Resume Incomplete",
            "Range: bytes=0-8388607\r\n",
            "",
        );
        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut s);
        assert!(headers.to_ascii_lowercase().contains(&format!(
            "content-range: bytes 8388608-{}/{total}",
            total - 1
        )));
        assert_eq!(body.len(), 10);
        assert!(body.iter().all(|b| *b == 7));
        reply(
            &mut s,
            "200 OK",
            "",
            r#"{"id":"created","name":"save","modifiedTime":"2026-01-01T00:00:00Z"}"#,
        );
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, vec![7; total]).unwrap();
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
async fn server_cannot_acknowledge_bytes_beyond_the_chunk_sent() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let url = Url::parse(&format!("http://{address}/upload")).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", "Location: /session\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut s);
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes 0-8388607/8388618")
        );
        assert_eq!(body.len(), 8 * 1024 * 1024);
        reply(
            &mut s,
            "308 Resume Incomplete",
            "Range: bytes=0-8388610\r\n",
            "",
        );
        listener.set_nonblocking(true).unwrap();
        listener
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, vec![7; 8 * 1024 * 1024 + 10]).unwrap();
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
async fn premature_completion_before_final_chunk_retains_session() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let url = Url::parse(&format!("http://{address}/upload")).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", "Location: /session\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut s);
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-range: bytes 0-8388607/8388618")
        );
        assert_eq!(body.len(), 8 * 1024 * 1024);
        reply(
            &mut s,
            "200 OK",
            "",
            r#"{"id":"premature","name":"save","modifiedTime":"2026-01-01T00:00:00Z"}"#,
        );
        listener.set_nonblocking(true).unwrap();
        listener
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, vec![7; 8 * 1024 * 1024 + 10]).unwrap();
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
        matches!(result, Err(UploadError::RecoveryFailed { cause, .. }) if matches!(*cause, UploadError::PrematureCompletion(_)))
    );
    let listener = server.join().unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn cross_origin_session_is_rejected_without_contacting_other_origin() {
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
    server.join().unwrap();
    assert_eq!(
        trap.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn session_url_rejects_userinfo_and_fragment() {
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
    use emusync_data_drive::DriveHttpPolicy;
    let trap = TcpListener::bind("127.0.0.1:0").unwrap();
    let trap_address = trap.local_addr().unwrap();
    trap.set_nonblocking(true).unwrap();
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
            "307 Temporary Redirect",
            &format!("Location: http://{trap_address}/trap\r\n"),
            "",
        );
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abc").unwrap();
    let clients = DriveHttpPolicy::default().build().unwrap();
    let result = ResumableUpload::with_clients(clients.metadata, clients.upload, url)
        .upload(
            "synthetic",
            &path,
            "save",
            "folder",
            &CancellationToken::new(),
        )
        .await;
    assert!(
        matches!(result, Err(UploadError::RecoveryFailed { cause, .. }) if matches!(*cause, UploadError::Http(StatusCode::TEMPORARY_REDIRECT)))
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
    let url = Url::parse(&format!("http://{address}/upload")).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", "Location: /session\r\n", "");
        for _ in 0..4 {
            let (mut s, _) = listener.accept().unwrap();
            read_request(&mut s);
            reply(&mut s, "308 Resume Incomplete", "", "");
        }
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
    assert!(
        matches!(result, Err(UploadError::RecoveryFailed { cause, .. }) if matches!(*cause, UploadError::NoProgress))
    );
    server.join().unwrap();
}

#[tokio::test]
async fn expired_session_restarts_once_and_uploads_empty_snapshot() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", "Location: /first\r\n", "");
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, _) = read_request(&mut stream);
        assert!(headers.starts_with("PUT /first"));
        reply(&mut stream, "404 Not Found", "", "");
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, _) = read_request(&mut stream);
        assert!(headers.starts_with("POST /upload?"));
        reply(&mut stream, "200 OK", "Location: /second\r\n", "");
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut stream);
        assert!(headers.starts_with("PUT /second"));
        assert!(headers.to_ascii_lowercase().contains("content-length: 0"));
        assert!(body.is_empty());
        reply(
            &mut stream,
            "200 OK",
            "",
            r#"{"id":"created","name":"save","modifiedTime":"2026-01-01T00:00:00Z"}"#,
        );
    });
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("empty");
    std::fs::write(&empty, []).unwrap();
    let file = ResumableUpload::new(Client::new(), url)
        .upload(
            "synthetic",
            &empty,
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
async fn invalid_server_range_is_rejected_without_resending_data() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", "Location: /session\r\n", "");
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "308 Resume Incomplete", "Range: 0-2\r\n", "");
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
async fn second_expired_session_stops_with_recovery_failed_instead_of_looping_forever() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        // First session: POST /upload -> 200 OK, PUT /session1 -> 404 Not Found
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", "Location: /session1\r\n", "");

        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "404 Not Found", "", "");

        // Second session: POST /upload -> 200 OK, PUT /session2 -> 404 Not Found
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", "Location: /session2\r\n", "");

        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "404 Not Found", "", "");

        listener.set_nonblocking(true).unwrap();
        listener
    });

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("save");
    std::fs::write(&path, b"test").unwrap();

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
        matches!(result, Err(UploadError::RecoveryFailed { cause, .. }) if matches!(*cause, UploadError::Http(StatusCode::NOT_FOUND)))
    );

    let listener = server.join().unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn regressive_or_malformed_308_range_in_multi_chunk_upload_is_rejected_without_resending() {
    // Subcase 1: Regressive range in multi-chunk upload (8 MiB + 10 bytes)
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            read_request(&mut s);
            reply(&mut s, "200 OK", "Location: /session\r\n", "");

            // First chunk of 8 MiB (8_388_608 bytes)
            let (mut s, _) = listener.accept().unwrap();
            let (headers, _) = read_request(&mut s);
            assert!(
                headers
                    .to_ascii_lowercase()
                    .contains("content-range: bytes 0-8388607/8388618")
            );
            reply(
                &mut s,
                "308 Resume Incomplete",
                "Range: bytes=0-8388607\r\n",
                "",
            );

            // Second chunk receives regressive range bytes=0-100
            let (mut s, _) = listener.accept().unwrap();
            let (headers, _) = read_request(&mut s);
            assert!(
                headers
                    .to_ascii_lowercase()
                    .contains("content-range: bytes 8388608-8388617/8388618")
            );
            reply(
                &mut s,
                "308 Resume Incomplete",
                "Range: bytes=0-100\r\n",
                "",
            );

            listener.set_nonblocking(true).unwrap();
            listener
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("multi_chunk");
        std::fs::write(&path, vec![0u8; 8 * 1024 * 1024 + 10]).unwrap();

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

    // Subcase 2: Malformed range missing "bytes=0-" prefix, e.g. "bytes=1-5"
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            read_request(&mut s);
            reply(&mut s, "200 OK", "Location: /session\r\n", "");

            let (mut s, _) = listener.accept().unwrap();
            read_request(&mut s);
            reply(&mut s, "308 Resume Incomplete", "Range: bytes=1-5\r\n", "");

            listener.set_nonblocking(true).unwrap();
            listener
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("save");
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
}
