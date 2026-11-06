use emusync_data_drive::DriveFolders;
use emusync_domain::sync::CloudFolderStore;
use reqwest::{Client, Url};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};
#[tokio::test]
async fn finds_root_creates_missing_child_and_moves_file() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        for i in 0..4 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 4096];
            let end = loop {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(pos) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                    break pos + 4;
                }
            };
            let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
            let length = headers
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|n| n.parse::<usize>().ok())
                })
                .unwrap_or(0);
            while bytes.len() < end + length {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
            }
            let body = &bytes[end..end + length];
            let response = match i {
                0 => {
                    assert!(headers.starts_with("GET "));
                    r#"{"files":[{"id":"root"}]}"#
                }
                1 => {
                    assert!(headers.starts_with("GET "));
                    r#"{"files":[]}"#
                }
                2 => {
                    assert!(headers.starts_with("POST "));
                    let body: serde_json::Value = serde_json::from_slice(body).unwrap();
                    assert_eq!(body["name"], "Game");
                    assert_eq!(body["parents"][0], "root");
                    r#"{"id":"child"}"#
                }
                _ => {
                    assert!(headers.starts_with("PATCH /files/file?"));
                    assert!(headers.contains("addParents=child"));
                    assert!(headers.contains("removeParents=root"));
                    "{}"
                }
            };
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
        }
    });
    let folders = DriveFolders::new(Client::new(), url);
    assert_eq!(
        folders
            .ensure_path("synthetic", &["Game".into()])
            .await
            .unwrap(),
        "child"
    );
    folders
        .move_file("synthetic", "file", "child", "root")
        .await
        .unwrap();
    server.join().unwrap();
}

fn read_request(stream: &mut std::net::TcpStream) -> (String, Vec<u8>) {
    let mut data = Vec::new();
    let mut buf = [0; 4096];
    let end = loop {
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0);
        data.extend_from_slice(&buf[..n]);
        if let Some(i) = data.windows(4).position(|v| v == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let headers = String::from_utf8(data[..end].to_vec()).unwrap();
    let length = headers
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .and_then(|v| v.parse::<usize>().ok())
        })
        .unwrap_or(0);
    while data.len() < end + length {
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0);
        data.extend_from_slice(&buf[..n]);
    }
    (headers, data[end..end + length].to_vec())
}
fn respond(stream: &mut std::net::TcpStream, status: &str, body: &str) {
    write!(stream,"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
}
#[tokio::test]
async fn folder_lookup_is_scoped_and_missing_child_does_not_create_it() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let (headers, _) = read_request(&mut s);
        assert!(headers.starts_with("GET "));
        respond(&mut s, "200 OK", r#"{"files":[{"id":"root"}]}"#);
        let (mut s, _) = listener.accept().unwrap();
        let (headers, _) = read_request(&mut s);
        let target = headers
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap();
        let parsed = Url::parse(&format!("http://localhost{target}")).unwrap();
        let params: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(
            params["q"],
            "name = 'O\\'Brien' and mimeType = 'application/vnd.google-apps.folder' and trashed = false and 'root' in parents"
        );
        respond(&mut s, "200 OK", r#"{"files":[]}"#);
        listener.set_nonblocking(true).unwrap();
        listener
    });
    let folders = DriveFolders::new(Client::new(), url);
    assert_eq!(
        folders
            .find_path("synthetic", &["O'Brien".into()])
            .await
            .unwrap(),
        None
    );
    let listener = server.join().unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
#[tokio::test]
async fn rename_and_explicit_delete_send_exact_requests() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut s);
        assert!(headers.starts_with("PATCH /files/id?"));
        assert!(headers.contains("addParents=new"));
        assert!(headers.contains("removeParents=old"));
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["name"], "O'Brien");
        respond(&mut s, "200 OK", "{}");
        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut s);
        assert!(headers.starts_with("DELETE /files/id "));
        assert!(body.is_empty());
        respond(&mut s, "204 No Content", "");
    });
    let folders = DriveFolders::new(Client::new(), url);
    folders
        .rename_folder("synthetic", "id", "O'Brien", Some("new"), Some("old"))
        .await
        .unwrap();
    folders.delete_folder("synthetic", "id").await.unwrap();
    server.join().unwrap();
}
#[tokio::test]
async fn lost_folder_create_response_is_unknown_and_never_retried_automatically() {
    use emusync_data_drive::{DriveHttpPolicy, FolderError};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        respond(&mut s, "200 OK", r#"{"files":[]}"#);
        let (mut s, _) = listener.accept().unwrap();
        let (headers, _) = read_request(&mut s);
        assert!(headers.starts_with("POST "));
        drop(s);
        listener.set_nonblocking(true).unwrap();
        listener
    });
    let folders = DriveFolders::new(DriveHttpPolicy::default().build().unwrap().metadata, url);
    assert!(matches!(
        folders.ensure_path("synthetic", &[]).await,
        Err(FolderError::MutationOutcomeUnknown(_))
    ));
    let listener = server.join().unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn blank_folder_id_never_authorizes_further_mutations() {
    use emusync_data_drive::{DriveHttpPolicy, FolderError};
    for existing in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let (headers, _) = read_request(&mut stream);
            assert!(headers.starts_with("GET "));
            if existing {
                respond(&mut stream, "200 OK", r#"{"files":[{"id":"  "}]}"#);
            } else {
                respond(&mut stream, "200 OK", r#"{"files":[]}"#);
                let (mut stream, _) = listener.accept().unwrap();
                let (headers, _) = read_request(&mut stream);
                assert!(headers.starts_with("POST "));
                respond(&mut stream, "200 OK", r#"{"id":"  "}"#);
            }
            listener.set_nonblocking(true).unwrap();
            listener
        });
        let client = DriveHttpPolicy::default().build().unwrap().metadata;
        let result = DriveFolders::new(client, url)
            .ensure_path("synthetic", &["child".into()])
            .await;
        if existing {
            assert!(matches!(result, Err(FolderError::InvalidFolderIdentity)));
        } else {
            assert!(matches!(result, Err(FolderError::MutationIdentityUnknown)));
        }
        assert_eq!(
            server.join().unwrap().accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unusable_folder_create_response_is_unknown_and_never_retried() {
    use emusync_data_drive::{FolderError, ResponseMetadataError};
    use std::{sync::mpsc, time::Duration};
    for oversized in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            read_request(&mut s);
            respond(&mut s, "200 OK", r#"{"files":[]}"#);
            let (mut s, _) = listener.accept().unwrap();
            let (headers, _) = read_request(&mut s);
            assert!(headers.starts_with("POST "));
            if oversized {
                let body = format!(r#"{{"id":"child","padding":"{}"}}"#, "a".repeat(2 << 20));
                respond(&mut s, "200 OK", &body);
            } else {
                write!(s, "HTTP/1.1 200 OK\r\nContent-Length: 40\r\n\r\n{{\"id\"").unwrap();
                let _ = release_rx.recv_timeout(Duration::from_secs(5));
            }
            listener.set_nonblocking(true).unwrap();
            listener
        });
        let folders = DriveFolders::new(Client::new(), url)
            .with_response_inactivity(Duration::from_millis(200));
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            folders.ensure_path("synthetic", &["child".into()]),
        )
        .await
        .unwrap();
        match result {
            Err(FolderError::MutationResponseUnknown(ResponseMetadataError::TooLarge))
                if oversized => {}
            Err(FolderError::MutationResponseUnknown(ResponseMetadataError::Inactive))
                if !oversized => {}
            other => panic!("unexpected result for oversized={oversized}: {other:?}"),
        }
        release_tx.send(()).ok();
        assert_eq!(
            server.join().unwrap().accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unusable_folder_search_response_is_a_read_failure_without_creating() {
    use emusync_data_drive::{FolderError, ResponseMetadataError};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        let body = format!(r#"{{"files":[],"padding":"{}"}}"#, "a".repeat(2 << 20));
        respond(&mut s, "200 OK", &body);
        listener.set_nonblocking(true).unwrap();
        listener
    });
    let result = DriveFolders::new(Client::new(), url)
        .ensure_path("synthetic", &["child".into()])
        .await;
    assert!(matches!(
        result,
        Err(FolderError::Metadata(ResponseMetadataError::TooLarge))
    ));
    assert_eq!(
        server.join().unwrap().accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
