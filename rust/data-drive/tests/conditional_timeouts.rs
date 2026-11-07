mod common;

use common::{read_request, reply};
use emusync_data_drive::{ConditionalError, ConditionalUpdate, FileRevision};
use reqwest::{Client, Url};
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    sync::Arc,
    thread,
};

fn metadata(etag: &str) -> String {
    format!(
        r#"{{"id":"file","title":"save.dat","modifiedDate":"2026-01-01T00:00:00Z","version":"1","etag":{etag}}}"#
    )
}

fn revision() -> FileRevision {
    FileRevision {
        id: "file".into(),
        version: Some("1".into()),
        sha256: None,
    }
}

fn adapter(listener: &TcpListener) -> ConditionalUpdate {
    let base = format!("http://{}/", listener.local_addr().unwrap());
    ConditionalUpdate::new(
        Client::new(),
        Url::parse(&format!("{base}files/")).unwrap(),
        Url::parse(&format!("{base}upload/")).unwrap(),
    )
}

#[tokio::test]
async fn pre_cancelled_token_aborts_before_get_or_snapshot_open() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let client = adapter(&listener);
    let cancel = tokio_util::sync::CancellationToken::new();
    cancel.cancel();

    let result = client
        .update_with_cancel(
            "synthetic",
            &revision(),
            Path::new("must-not-open"),
            "save.dat",
            Arc::new(|_| {}),
            &cancel,
        )
        .await;

    assert!(matches!(result, Err(ConditionalError::Cancelled)));
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_during_metadata_read_prevents_conditional_put() {
    use std::{sync::mpsc, time::Duration};
    use tokio_util::sync::CancellationToken;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = adapter(&listener);
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, _) = read_request(&mut stream);
        assert!(headers.starts_with("GET "));
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(stream);
        listener.set_nonblocking(true).unwrap();
        listener
    });
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("save");
    std::fs::write(&path, b"abc").unwrap();
    let cancel = CancellationToken::new();
    let worker_cancel = cancel.clone();
    let task = tokio::spawn(async move {
        client
            .update_with_cancel(
                "synthetic",
                &revision(),
                &path,
                "save.dat",
                Arc::new(|_| {}),
                &worker_cancel,
            )
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    cancel.cancel();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap(),
        Err(ConditionalError::Cancelled)
    ));
    release_tx.send(()).unwrap();
    let listener = server.join().unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_during_metadata_body_prevents_conditional_put() {
    use std::{sync::mpsc, time::Duration};
    use tokio_util::sync::CancellationToken;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = adapter(&listener);
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{{"
        )
        .unwrap();
        stream.flush().unwrap();
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(stream);
        listener.set_nonblocking(true).unwrap();
        listener
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("save");
    std::fs::write(&path, b"abc").unwrap();
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    let task = tokio::spawn(async move {
        client
            .update_with_cancel(
                "synthetic",
                &revision(),
                &path,
                "save.dat",
                Arc::new(|_| {}),
                &signal,
            )
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    cancel.cancel();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap(),
        Err(ConditionalError::Cancelled)
    ));
    release_tx.send(()).unwrap();
    assert_eq!(
        server.join().unwrap().accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn oversized_read_only_metadata_never_starts_conditional_put() {
    use emusync_data_drive::ResponseMetadataError;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = adapter(&listener);
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        let body = vec![b'x'; 1024 * 1024 + 1];
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        let _ = stream.write_all(&body);
        drop(stream);
        listener.set_nonblocking(true).unwrap();
        listener
    });
    let result = client
        .update(
            "synthetic",
            &revision(),
            Path::new("not-opened"),
            "save.dat",
        )
        .await;
    assert!(matches!(
        result,
        Err(ConditionalError::ControlMetadata(
            ResponseMetadataError::TooLarge
        ))
    ));
    assert_eq!(
        server.join().unwrap().accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_after_put_waits_for_the_remote_response() {
    use std::{sync::mpsc, time::Duration};
    use tokio_util::sync::CancellationToken;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = adapter(&listener);
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", &metadata(r#""\"v1\"""#));
        let (mut s, _) = listener.accept().unwrap();
        let (_, body) = read_request(&mut s);
        assert_eq!(body, b"abc");
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        reply(&mut s, "200 OK", &metadata(r#""\"v2\"""#));
    });
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("save");
    std::fs::write(&path, b"abc").unwrap();
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    let mut task = tokio::spawn(async move {
        client
            .update_with_cancel(
                "synthetic",
                &revision(),
                &path,
                "save.dat",
                Arc::new(|_| {}),
                &signal,
            )
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    cancel.cancel();
    assert!(
        tokio::time::timeout(Duration::from_millis(25), &mut task)
            .await
            .is_err()
    );
    release_tx.send(()).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result.id, "file");
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stalled_conditional_body_waits_and_completes_when_receiver_resumes() {
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = adapter(&listener);
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", &metadata(r#""\"v1\"""#));
        let (mut s, _) = listener.accept().unwrap();
        let mut headers = Vec::new();
        let mut one = [0; 1];
        while !headers.ends_with(b"\r\n\r\n") {
            assert_eq!(s.read(&mut one).unwrap(), 1);
            headers.push(one[0]);
            assert!(headers.len() < 8192);
        }
        assert!(String::from_utf8_lossy(&headers).starts_with("PUT /upload/file?"));
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let mut remaining = 16 * 1024 * 1024;
        let mut buf = [0; 65536];
        while remaining > 0 {
            let count = s.read(&mut buf[..remaining.min(65536)]).unwrap();
            assert!(count > 0);
            remaining -= count;
        }
        reply(&mut s, "200 OK", &metadata(r#""\"v2\"""#));
    });
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("save");
    std::fs::write(&path, vec![7; 16 * 1024 * 1024]).unwrap();
    let mut task = tokio::spawn(async move {
        client
            .update("synthetic", &revision(), &path, "save.dat")
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(150), &mut task)
            .await
            .is_err()
    );
    release_tx.send(()).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result.id, "file");
    server.join().unwrap();
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn linux_tcp_timeout_stops_stalled_conditional_body_without_cancellation() {
    use emusync_data_drive::DriveHttpPolicy;
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let clients = DriveHttpPolicy {
        connect: Duration::from_secs(2),
        metadata: Duration::from_secs(2),
        inactivity: Duration::from_millis(500),
    }
    .build()
    .unwrap();
    let client = ConditionalUpdate::with_clients(
        clients.metadata,
        clients.upload,
        Url::parse(&format!("{root}files/")).unwrap(),
        Url::parse(&format!("{root}upload/")).unwrap(),
    );
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", &metadata(r#""\"v1\"""#));
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
        release_rx.recv_timeout(Duration::from_secs(15)).unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, vec![7; 16 * 1024 * 1024]).unwrap();
    let task = tokio::spawn(async move {
        client
            .update("synthetic", &revision(), &path, "save.dat")
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(result, Err(ConditionalError::OutcomeUnknown(_))));
    release_tx.send(()).unwrap();
    server.join().unwrap();
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn linux_tcp_timeout_does_not_bound_silent_response_after_body_delivery() {
    use emusync_data_drive::DriveHttpPolicy;
    use std::{sync::mpsc, time::Duration};
    use tokio_util::sync::CancellationToken;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let clients = DriveHttpPolicy {
        connect: Duration::from_secs(2),
        metadata: Duration::from_secs(2),
        inactivity: Duration::from_millis(150),
    }
    .build()
    .unwrap();
    let client = ConditionalUpdate::with_clients(
        clients.metadata,
        clients.upload,
        Url::parse(&format!("{root}files/")).unwrap(),
        Url::parse(&format!("{root}upload/")).unwrap(),
    );
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", &metadata(r#""\"v1\"""#));
        let (mut stream, _) = listener.accept().unwrap();
        assert_eq!(read_request(&mut stream).1, b"abc");
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abc").unwrap();
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    let mut task = tokio::spawn(async move {
        client
            .update_with_cancel(
                "synthetic",
                &revision(),
                &path,
                "save.dat",
                Arc::new(|_| {}),
                &signal,
            )
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(600), &mut task)
            .await
            .is_err()
    );
    cancel.cancel();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(4), task)
            .await
            .unwrap()
            .unwrap(),
        Err(ConditionalError::CancelledOutcomeUnknown)
    ));
    release_tx.send(()).unwrap();
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn empty_conditional_upload_times_out_waiting_for_response_headers() {
    use emusync_data_drive::DriveHttpPolicy;
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let clients = DriveHttpPolicy {
        connect: Duration::from_secs(2),
        metadata: Duration::from_secs(2),
        inactivity: Duration::from_millis(200),
    }
    .build()
    .unwrap();
    let client = ConditionalUpdate::with_transfer_clients(
        clients.metadata,
        clients.upload,
        clients.empty_upload,
        Url::parse(&format!("{root}files/")).unwrap(),
        Url::parse(&format!("{root}upload/")).unwrap(),
    );
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", &metadata(r#""\"v1\"""#));
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut stream);
        assert!(headers.to_ascii_lowercase().contains("content-length: 0"));
        assert!(body.is_empty());
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(stream);
        listener.set_nonblocking(true).unwrap();
        listener
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty");
    std::fs::write(&path, []).unwrap();
    let task = tokio::spawn(async move {
        client
            .update("synthetic", &revision(), &path, "save.dat")
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap(),
        Err(ConditionalError::OutcomeUnknown(_))
    ));
    release_tx.send(()).unwrap();
    assert_eq!(
        server.join().unwrap().accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nonempty_conditional_upload_times_out_after_body_with_unknown_outcome() {
    use emusync_data_drive::DriveHttpPolicy;
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let clients = DriveHttpPolicy {
        connect: Duration::from_secs(2),
        metadata: Duration::from_secs(2),
        inactivity: Duration::from_millis(200),
    }
    .build()
    .unwrap();
    let client = ConditionalUpdate::with_transfer_clients(
        clients.metadata,
        clients.upload.clone(),
        clients.upload,
        Url::parse(&format!("{root}files/")).unwrap(),
        Url::parse(&format!("{root}upload/")).unwrap(),
    )
    .with_response_inactivity(Duration::from_millis(200));
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", &metadata(r#""\"v1\"""#));
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut stream);
        assert!(headers.to_ascii_lowercase().contains("content-length: 3"));
        assert_eq!(body, b"abc");
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(stream);
        listener.set_nonblocking(true).unwrap();
        listener
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nonempty");
    std::fs::write(&path, b"abc").unwrap();
    let task = tokio::spawn(async move {
        client
            .update("synthetic", &revision(), &path, "save.dat")
            .await
    });
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap(),
        Err(ConditionalError::HeadersTimedOut)
    ));
    release_tx.send(()).unwrap();
    assert_eq!(
        server.join().unwrap().accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the local openssl executable to generate an ephemeral synthetic certificate"]
async fn linux_tcp_timeout_distinguishes_stalled_and_active_tls_uploads() {
    use emusync_data_drive::DriveHttpPolicy;
    use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
    use std::{process::Command, sync::mpsc, time::Duration};
    let dir = tempfile::tempdir().unwrap();
    let key_pem = dir.path().join("key.pem");
    let cert_pem = dir.path().join("cert.pem");
    let key_der = dir.path().join("key.der");
    let cert_der = dir.path().join("cert.der");
    let command = Command::new("openssl")
        .args(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout"])
        .arg(&key_pem)
        .arg("-out")
        .arg(&cert_pem)
        .args([
            "-days",
            "2",
            "-subj",
            "/CN=EmuSync-Synthetic",
            "-addext",
            "subjectAltName=IP:127.0.0.1",
            "-addext",
            "basicConstraints=critical,CA:FALSE",
        ])
        .output()
        .unwrap();
    assert!(command.status.success());
    assert!(
        Command::new("openssl")
            .args(["x509", "-in"])
            .arg(&cert_pem)
            .args(["-outform", "DER", "-out"])
            .arg(&cert_der)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        Command::new("openssl")
            .args([
                "pkcs8", "-topk8", "-inform", "PEM", "-outform", "DER", "-in"
            ])
            .arg(&key_pem)
            .arg("-out")
            .arg(&key_der)
            .arg("-nocrypt")
            .output()
            .unwrap()
            .status
            .success()
    );
    let cert = std::fs::read(&cert_der).unwrap();
    let key = std::fs::read(&key_der).unwrap();
    let mut server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(cert.clone())],
            PrivatePkcs8KeyDer::from(key).into(),
        )
        .unwrap();
    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    let server_config = Arc::new(server_config);
    for stalled in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let root = format!("https://{}/", listener.local_addr().unwrap());
        let (seen_tx, seen_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let server_config = server_config.clone();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut tls = rustls::StreamOwned::new(
                rustls::ServerConnection::new(server_config.clone()).unwrap(),
                stream,
            );
            read_request(&mut tls);
            reply(&mut tls, "200 OK", &metadata(r#""\"v1\"""#));
            tls.flush().unwrap();
            drop(tls);
            let (stream, _) = listener.accept().unwrap();
            if stalled {
                socket2::SockRef::from(&stream)
                    .set_recv_buffer_size(4096)
                    .unwrap();
            }
            let mut tls = rustls::StreamOwned::new(
                rustls::ServerConnection::new(server_config).unwrap(),
                stream,
            );
            let mut headers = Vec::new();
            let mut byte = [0; 1];
            while !headers.ends_with(b"\r\n\r\n") {
                assert_eq!(tls.read(&mut byte).unwrap(), 1);
                headers.push(byte[0]);
            }
            assert_eq!(tls.conn.alpn_protocol(), Some(b"http/1.1".as_slice()));
            if stalled {
                seen_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(15)).unwrap();
            } else {
                let mut remaining = 4 * 1024 * 1024;
                let mut bytes = [0; 16 * 1024];
                let start = std::time::Instant::now();
                while remaining > 0 {
                    let limit = remaining.min(bytes.len());
                    let count = tls.read(&mut bytes[..limit]).unwrap();
                    assert!(count > 0);
                    remaining -= count;
                    thread::sleep(Duration::from_millis(5));
                }
                assert!(start.elapsed() > Duration::from_millis(500));
                reply(&mut tls, "200 OK", &metadata(r#""\"v2\"""#));
                tls.flush().unwrap();
            }
        });
        let trusted = reqwest::Certificate::from_der(&cert).unwrap();
        let clients = DriveHttpPolicy {
            connect: Duration::from_secs(2),
            metadata: Duration::from_secs(2),
            inactivity: Duration::from_millis(500),
        }
        .build_with(|| Client::builder().add_root_certificate(trusted.clone()))
        .unwrap();
        let client = ConditionalUpdate::with_clients(
            clients.metadata,
            clients.upload,
            Url::parse(&format!("{root}files/")).unwrap(),
            Url::parse(&format!("{root}upload/")).unwrap(),
        );
        let snapshot = dir.path().join("snapshot");
        let size = if stalled {
            16 * 1024 * 1024
        } else {
            4 * 1024 * 1024
        };
        std::fs::write(&snapshot, vec![7; size]).unwrap();
        let task = tokio::spawn(async move {
            client
                .update("synthetic", &revision(), &snapshot, "save.dat")
                .await
        });
        if stalled {
            seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        let result = tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .unwrap()
            .unwrap();
        if stalled {
            assert!(matches!(result, Err(ConditionalError::OutcomeUnknown(_))));
            release_tx.send(()).unwrap();
        } else {
            assert_eq!(result.unwrap().id, "file");
        }
        server.join().unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_of_stalled_conditional_body_reports_unknown_outcome() {
    use std::{sync::mpsc, time::Duration};
    use tokio_util::sync::CancellationToken;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = adapter(&listener);
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", &metadata(r#""\"v1\"""#));
        let (mut stream, _) = listener.accept().unwrap();
        let mut headers = Vec::new();
        let mut byte = [0; 1];
        while !headers.ends_with(b"\r\n\r\n") {
            assert_eq!(stream.read(&mut byte).unwrap(), 1);
            headers.push(byte[0]);
        }
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, vec![7; 16 * 1024 * 1024]).unwrap();
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    let task = tokio::spawn(async move {
        client
            .update_with_cancel(
                "synthetic",
                &revision(),
                &path,
                "save.dat",
                Arc::new(|_| {}),
                &signal,
            )
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
        Err(ConditionalError::CancelledOutcomeUnknown)
    ));
    release_tx.send(()).unwrap();
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_of_stalled_success_metadata_reports_unknown_outcome() {
    use std::{sync::mpsc, time::Duration};
    use tokio_util::sync::CancellationToken;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = adapter(&listener);
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", &metadata(r#""\"v1\"""#));
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
        client
            .update_with_cancel(
                "synthetic",
                &revision(),
                &path,
                "save.dat",
                Arc::new(|_| {}),
                &signal,
            )
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
        Err(ConditionalError::CancelledOutcomeUnknown)
    ));
    release_tx.send(()).unwrap();
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stalled_success_metadata_times_out_with_unknown_outcome() {
    use std::{sync::mpsc, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = adapter(&listener).with_response_inactivity(Duration::from_millis(150));
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", &metadata(r#""\"v1\"""#));
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{{"
        )
        .unwrap();
        stream.flush().unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abc").unwrap();
    let result = client
        .update("synthetic", &revision(), &path, "save.dat")
        .await;
    assert!(matches!(
        result,
        Err(ConditionalError::ResponseMetadata(
            emusync_data_drive::ResponseMetadataError::Inactive
        ))
    ));
    release_tx.send(()).unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn oversized_success_metadata_preserves_unknown_outcome() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = adapter(&listener);
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", &metadata(r#""\"v1\"""#));
        let (mut stream, _) = listener.accept().unwrap();
        let (_, body) = read_request(&mut stream);
        assert_eq!(body, b"abc");
        let body = vec![b'x'; 1024 * 1024 + 1];
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        let _ = stream.write_all(&body);
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abc").unwrap();
    let result = client
        .update("synthetic", &revision(), &path, "save.dat")
        .await;
    assert!(matches!(
        result,
        Err(ConditionalError::ResponseMetadata(
            emusync_data_drive::ResponseMetadataError::TooLarge
        ))
    ));
    server.join().unwrap();
}

#[tokio::test]
async fn active_success_metadata_can_exceed_inactivity_interval() {
    use std::time::Duration;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = adapter(&listener).with_response_inactivity(Duration::from_millis(150));
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        reply(&mut stream, "200 OK", &metadata(r#""\"v1\"""#));
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        let body = metadata(r#""\"v2\"""#);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        for chunk in body.as_bytes().chunks(body.len().div_ceil(4)) {
            thread::sleep(Duration::from_millis(80));
            stream.write_all(chunk).unwrap();
            stream.flush().unwrap();
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"abc").unwrap();
    assert_eq!(
        client
            .update("synthetic", &revision(), &path, "save.dat")
            .await
            .unwrap()
            .id,
        "file"
    );
    server.join().unwrap();
}
