mod common;

use common::{read_request, reply};
use emusync_data_drive::{
    ConditionalError, ConditionalUpdate, FileRevision, ResponseMetadataError,
};
use reqwest::{Client, StatusCode, Url};
use std::{
    net::TcpListener,
    path::Path,
    sync::{Arc, Barrier, Mutex},
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

#[tokio::test(flavor = "multi_thread")]
async fn simultaneous_writers_use_if_match_and_only_one_replaces_the_remote() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = adapter(&listener);
    let barrier = Arc::new(Barrier::new(2));
    let stored = Arc::new(Mutex::new(None::<Vec<u8>>));
    let server_stored = stored.clone();
    let server = thread::spawn(move || {
        let mut workers = Vec::new();
        for _ in 0..4 {
            let (mut stream, _) = listener.accept().unwrap();
            let barrier = barrier.clone();
            let stored = server_stored.clone();
            workers.push(thread::spawn(move || {
                let (headers, body) = read_request(&mut stream);
                if headers.starts_with("GET ") {
                    assert!(headers.starts_with("GET /files/file?"));
                    barrier.wait();
                    reply(&mut stream, "200 OK", &metadata(r#""\"v1\"""#));
                } else {
                    assert!(headers.starts_with("PUT /upload/file?"));
                    assert!(
                        headers
                            .to_ascii_lowercase()
                            .contains("if-match: \"v1\"\r\n")
                    );
                    let mut value = stored.lock().unwrap();
                    if value.is_none() {
                        *value = Some(body);
                        reply(&mut stream, "200 OK", &metadata(r#""\"v2\"""#));
                    } else {
                        reply(&mut stream, "412 Precondition Failed", "{}");
                    }
                }
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }
    });
    let temp = tempfile::tempdir().unwrap();
    let one = temp.path().join("one");
    let two = temp.path().join("two");
    std::fs::write(&one, b"first").unwrap();
    std::fs::write(&two, b"second").unwrap();
    let observed = revision();
    let progress = Arc::new(Mutex::new(Vec::new()));
    let recorded = progress.clone();
    let (a, b) = tokio::join!(
        client.update_with_progress(
            "synthetic",
            &observed,
            &one,
            "save.dat",
            Arc::new(move |event| recorded.lock().unwrap().push(event))
        ),
        client.update("synthetic", &observed, &two, "save.dat")
    );
    assert!(matches!(
        (&a, &b),
        (Ok(_), Err(ConditionalError::Changed)) | (Err(ConditionalError::Changed), Ok(_))
    ));
    server.join().unwrap();
    let events = progress.lock().unwrap();
    let event = events.last().unwrap();
    assert_eq!(
        (event.name.as_str(), event.transferred, event.total),
        ("one", 5, Some(5))
    );
    assert!(matches!(
        stored.lock().unwrap().as_deref(),
        Some(b"first") | Some(b"second")
    ));
}

#[tokio::test]
async fn missing_weak_wildcard_and_malformed_etags_never_authorize_update() {
    for etag in [
        None,
        Some(""),
        Some("*"),
        Some("W/\"weak\""),
        Some("unquoted"),
        Some("\"line\rbreak\""),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = adapter(&listener);
        let value = serde_json::to_string(&etag).unwrap();
        let body = if etag.is_none() {
            metadata(&value).replace(",\"etag\":null", "")
        } else {
            metadata(&value)
        };
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let (headers, _) = read_request(&mut stream);
            assert!(headers.starts_with("GET "));
            reply(&mut stream, "200 OK", &body);
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
        assert!(
            matches!(result, Err(ConditionalError::MissingStrongEtag)),
            "etag {etag:?}"
        );
        assert_eq!(
            server.join().unwrap().accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}

#[tokio::test]
async fn stale_revision_is_rejected_before_opening_snapshot_or_sending_put() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = adapter(&listener);
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, _) = read_request(&mut stream);
        assert!(headers.starts_with("GET "));
        let body = metadata(r#""\"v1\"""#).replace(r#""version":"1""#, r#""version":"2""#);
        reply(&mut stream, "200 OK", &body);
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
    assert!(matches!(result, Err(ConditionalError::Changed)));
    let listener = server.join().unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn lost_conditional_write_response_is_an_unknown_outcome() {
    use emusync_data_drive::DriveHttpPolicy;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    let clients = DriveHttpPolicy::default().build().unwrap();
    let client = ConditionalUpdate::with_clients(
        clients.metadata,
        clients.upload,
        Url::parse(&format!("{root}files/")).unwrap(),
        Url::parse(&format!("{root}upload/")).unwrap(),
    );
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", &metadata(r#""\"v1\"""#));
        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut s);
        assert!(headers.to_ascii_lowercase().contains("if-match: \"v1\""));
        assert_eq!(body, b"abc");
        drop(s);
        listener.set_nonblocking(true).unwrap();
        listener
    });
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("save");
    std::fs::write(&path, b"abc").unwrap();
    assert!(matches!(
        client
            .update("synthetic", &revision(), &path, "save.dat")
            .await,
        Err(ConditionalError::OutcomeUnknown(_))
    ));
    let listener = server.join().unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn server_error_after_conditional_put_is_not_assumed_to_be_unwritten() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = adapter(&listener);
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", &metadata(r#""\"v1\"""#));
        let (mut s, _) = listener.accept().unwrap();
        let (_, body) = read_request(&mut s);
        assert_eq!(body, b"abc");
        reply(&mut s, "503 Service Unavailable", "{}");
        listener.set_nonblocking(true).unwrap();
        listener
    });
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("save");
    std::fs::write(&path, b"abc").unwrap();
    assert!(matches!(
        client
            .update("synthetic", &revision(), &path, "save.dat")
            .await,
        Err(ConditionalError::ServerOutcomeUnknown(
            reqwest::StatusCode::SERVICE_UNAVAILABLE
        ))
    ));
    let listener = server.join().unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn unexpected_response_retains_reported_identity_for_recovery() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = adapter(&listener);
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", &metadata(r#""\"v1\"""#));
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        let response = metadata(r#""\"v2\"""#).replace("save.dat", "different.dat");
        reply(&mut s, "200 OK", &response);
    });
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("save");
    std::fs::write(&path, b"abc").unwrap();
    assert!(
        matches!(client.update("synthetic",&revision(),&path,"save.dat").await,Err(ConditionalError::UnexpectedResponse(file)) if file.title=="different.dat" && file.id=="file")
    );
    server.join().unwrap();
}

#[tokio::test]
async fn matching_version_with_divergent_sha256_or_renamed_remote_title_is_rejected_before_put() {
    let hostile_responses = [
        (
            FileRevision {
                id: "file".into(),
                version: Some("1".into()),
                sha256: Some("hash1".into()),
            },
            r#"{"id":"file","title":"save.dat","modifiedDate":"2026-01-01T00:00:00Z","version":"1","sha256Checksum":"hash2","etag":"\"v1\""}"#,
        ),
        (
            FileRevision {
                id: "file".into(),
                version: Some("1".into()),
                sha256: None,
            },
            r#"{"id":"file","title":"renamed.dat","modifiedDate":"2026-01-01T00:00:00Z","version":"1","etag":"\"v1\""}"#,
        ),
        (
            FileRevision {
                id: "file".into(),
                version: Some("1".into()),
                sha256: None,
            },
            r#"{"id":"other-id","title":"save.dat","modifiedDate":"2026-01-01T00:00:00Z","version":"1","etag":"\"v1\""}"#,
        ),
    ];

    for (observed, server_metadata) in hostile_responses {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = adapter(&listener);
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            read_request(&mut s);
            reply(&mut s, "200 OK", server_metadata);
            listener.set_nonblocking(true).unwrap();
            listener
        });

        let result = client
            .update("synthetic", &observed, Path::new("not-opened"), "save.dat")
            .await;

        assert!(
            matches!(result, Err(ConditionalError::Changed)),
            "expected Changed for hostile metadata: {server_metadata}"
        );
        let listener = server.join().unwrap();
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = adapter(&listener);
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let (headers, _) = read_request(&mut s);
        assert!(headers.starts_with("GET "));
        let get_response = r#"{"id":"file","title":"save.dat","modifiedDate":"2026-01-01T00:00:00Z","version":null,"sha256Checksum":"abcdef","etag":"\"v1\""}"#;
        reply(&mut s, "200 OK", get_response);

        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut s);
        assert!(headers.starts_with("PUT /upload/file?"));
        assert!(headers.to_ascii_lowercase().contains("if-match: \"v1\""));
        assert_eq!(body, b"test-save");
        let put_response = r#"{"id":"file","title":"save.dat","modifiedDate":"2026-01-01T00:00:00Z","version":"2","sha256Checksum":"abcdef","etag":"\"v2\""}"#;
        reply(&mut s, "200 OK", put_response);
    });

    let dir = tempfile::tempdir().unwrap();
    let save_path = dir.path().join("save.dat");
    std::fs::write(&save_path, b"test-save").unwrap();

    let observed_case_insensitive = FileRevision {
        id: "file".into(),
        version: None,
        sha256: Some("ABCDEF".into()),
    };

    let result = client
        .update(
            "synthetic",
            &observed_case_insensitive,
            &save_path,
            "save.dat",
        )
        .await
        .unwrap();

    assert_eq!(result.id, "file");
    assert_eq!(result.version.as_deref(), Some("2"));
    server.join().unwrap();
}

#[tokio::test]
async fn revision_without_version_or_hash_and_invalid_file_ids_abort_before_network() {
    let invalid_ids = ["", "  ", ".", "..", "a/b", "a\\b", "a\0b"];
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let client = adapter(&listener);

    for id in invalid_ids {
        let revision = FileRevision {
            id: id.into(),
            version: Some("1".into()),
            sha256: None,
        };
        let result = client
            .update("synthetic", &revision, Path::new("not-opened"), "save.dat")
            .await;
        assert!(
            matches!(result, Err(ConditionalError::InvalidEndpoint)),
            "expected InvalidEndpoint for id: {id:?}"
        );
    }

    let revision_no_ver_no_hash = FileRevision {
        id: "file".into(),
        version: Some("  ".into()),
        sha256: None,
    };
    let result = client
        .update(
            "synthetic",
            &revision_no_ver_no_hash,
            Path::new("not-opened"),
            "save.dat",
        )
        .await;
    assert!(matches!(result, Err(ConditionalError::Changed)));

    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn get_not_found_and_put_conflict_or_not_found_map_to_changed_while_get_http_error_is_preserved()
 {
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = adapter(&listener);
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            read_request(&mut s);
            reply(&mut s, "404 Not Found", "{}");
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
        assert!(matches!(result, Err(ConditionalError::Changed)));
        let listener = server.join().unwrap();
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = adapter(&listener);
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            read_request(&mut s);
            reply(&mut s, "503 Service Unavailable", "{}");
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
            Err(ConditionalError::Http(StatusCode::SERVICE_UNAVAILABLE))
        ));
        let listener = server.join().unwrap();
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    for status_str in ["404 Not Found", "409 Conflict"] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = adapter(&listener);
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            read_request(&mut s);
            reply(&mut s, "200 OK", &metadata(r#""\"v1\"""#));

            let (mut s, _) = listener.accept().unwrap();
            let (headers, body) = read_request(&mut s);
            assert!(headers.starts_with("PUT /upload/file?"));
            assert_eq!(body, b"save-bytes");
            reply(&mut s, status_str, "{}");
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("save.dat");
        std::fs::write(&path, b"save-bytes").unwrap();

        let result = client
            .update("synthetic", &revision(), &path, "save.dat")
            .await;
        assert!(
            matches!(result, Err(ConditionalError::Changed)),
            "expected Changed for PUT response {status_str}"
        );
        server.join().unwrap();
    }
}

#[tokio::test]
async fn invalid_json_in_put_response_is_classified_as_uncertain_response_metadata() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = adapter(&listener);
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        reply(&mut s, "200 OK", &metadata(r#""\"v1\"""#));

        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut s);
        assert!(headers.starts_with("PUT /upload/file?"));
        assert_eq!(body, b"save-bytes");
        reply(&mut s, "200 OK", "{not-json}");
    });

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("save.dat");
    std::fs::write(&path, b"save-bytes").unwrap();

    let result = client
        .update("synthetic", &revision(), &path, "save.dat")
        .await;

    assert!(matches!(
        result,
        Err(ConditionalError::ResponseMetadata(
            ResponseMetadataError::InvalidJson(_)
        ))
    ));
    server.join().unwrap();
}
