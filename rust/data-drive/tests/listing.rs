use emusync_data_drive::DriveListing;
use emusync_domain::sync::CloudFileListing;
use reqwest::{Client, Url};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

#[tokio::test]
async fn listing_follows_pages_and_filters_folders_with_precise_timestamps() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        for i in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let count = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..count]);
            if i == 1 {
                assert!(request.contains("pageToken=next"));
            }
            let body = if i == 0 {
                r#"{"nextPageToken":"next","files":[{"id":"folder","name":"nested","modifiedTime":"2026-01-01T00:00:00Z","mimeType":"application/vnd.google-apps.folder"}]}"#
            } else {
                r#"{"files":[{"id":"file","name":"save","modifiedTime":"1970-01-01T00:00:00.123456789Z","version":"2"}]}"#
            };
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
    });
    let listing = DriveListing::new(Client::new(), url);
    let files = listing.list_files("synthetic", "folder").await.unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].id, "file");
    assert_eq!(files[0].modified, 123_456_789);
    assert_eq!(files[0].version.as_deref(), Some("2"));
    server.join().unwrap();
}

#[tokio::test]
async fn migration_listing_includes_folders_and_files_across_pages() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        for i in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let count = stream.read(&mut request).unwrap();
            if i == 1 {
                assert!(String::from_utf8_lossy(&request[..count]).contains("pageToken=next"));
            }
            let body = if i == 0 {
                r#"{"nextPageToken":"next","files":[{"id":"folder","name":"nested","modifiedTime":"2026-01-01T00:00:00Z","mimeType":"application/vnd.google-apps.folder"}]}"#
            } else {
                r#"{"files":[{"id":"file","name":"save","modifiedTime":"2026-01-01T00:00:00Z","mimeType":"application/octet-stream"}]}"#
            };
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
    });
    let children = DriveListing::new(Client::new(), url)
        .list_children("synthetic", "folder")
        .await
        .unwrap();
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].id, "folder");
    assert!(children[0].is_folder);
    assert_eq!(children[1].id, "file");
    assert!(!children[1].is_folder);
    server.join().unwrap();
}

#[tokio::test]
async fn exact_search_escapes_query_and_requests_latest_match() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        let count = stream.read(&mut request).unwrap();
        let request = String::from_utf8_lossy(&request[..count]);
        let target = request
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap();
        let url = Url::parse(&format!("http://localhost{target}")).unwrap();
        let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(
            params["q"],
            "name = 'O\\'Brien.sav' and trashed = false and 'folder' in parents"
        );
        assert_eq!(params["orderBy"], "modifiedTime desc");
        assert_eq!(params["pageSize"], "1");
        let body = r#"{"files":[{"id":"latest","name":"O'Brien.sav","modifiedTime":"2026-01-01T00:00:00Z"}]}"#;
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
    });
    let listing = DriveListing::new(Client::new(), url);
    assert_eq!(
        listing
            .find_by_name("synthetic", "O'Brien.sav", Some("folder"))
            .await
            .unwrap()
            .unwrap()
            .id,
        "latest"
    );
    server.join().unwrap();
}

#[tokio::test]
async fn listing_rejects_file_without_usable_drive_id() {
    use emusync_data_drive::ListingError;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        assert!(stream.read(&mut request).unwrap() > 0);
        let body = r#"{"files":[{"id":"  ","name":"save","modifiedTime":"2026-01-01T00:00:00Z"}]}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let result = DriveListing::new(Client::new(), url)
        .list_files("synthetic", "folder")
        .await;
    assert!(matches!(result, Err(ListingError::InvalidIdentity)));
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn oversized_or_stalled_listing_pages_fail_without_partial_results() {
    use emusync_data_drive::{ListingError, ResponseMetadataError};
    use std::{sync::mpsc, time::Duration};
    for oversized in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request).unwrap();
            if oversized {
                let body = format!(r#"{{"files":[],"padding":"{}"}}"#, "a".repeat(2 << 20));
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            } else {
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: 40\r\n\r\n{{\"files\""
                )
                .unwrap();
                let _ = release_rx.recv_timeout(Duration::from_secs(5));
            }
        });
        let listing = DriveListing::new(Client::new(), url)
            .with_response_inactivity(Duration::from_millis(200));
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            listing.list_files("synthetic", "folder"),
        )
        .await
        .unwrap();
        match result {
            Err(ListingError::Metadata(ResponseMetadataError::TooLarge)) if oversized => {}
            Err(ListingError::Metadata(ResponseMetadataError::Inactive)) if !oversized => {}
            other => panic!("unexpected result for oversized={oversized}: {other:?}"),
        }
        release_tx.send(()).ok();
        server.join().unwrap();
    }
}

#[tokio::test]
async fn get_by_id_retrieves_exact_file() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        let n = stream.read(&mut request).unwrap();
        let req = String::from_utf8_lossy(&request[..n]);
        assert!(req.starts_with("GET /files/target_file?"));
        let body = r#"{"id":"target_file","name":"save.dat","modifiedTime":"2026-01-01T00:00:00Z","mimeType":"application/octet-stream","sha256Checksum":"abcdef","version":"5"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let listing = DriveListing::new(Client::new(), url);
    let file = listing.get_by_id("synthetic", "target_file").await.unwrap();
    assert_eq!(file.id, "target_file");
    assert_eq!(file.name, "save.dat");
    assert_eq!(file.sha256.as_deref(), Some("abcdef"));
    assert_eq!(file.version.as_deref(), Some("5"));
    assert!(!file.is_folder);
    server.join().unwrap();
}

#[tokio::test]
async fn get_by_id_in_parent_verifies_parent_presence() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        for i in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request).unwrap();
            let body = if i == 0 {
                r#"{"id":"file1","name":"save1.dat","modifiedTime":"2026-01-01T00:00:00Z","parents":["parent_folder"]}"#
            } else {
                r#"{"id":"file1","name":"save1.dat","modifiedTime":"2026-01-01T00:00:00Z","parents":["other_folder"]}"#
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    let listing = DriveListing::new(Client::new(), url);

    // 1. Matches expected parent
    let matched = listing
        .get_by_id_in_parent("synthetic", "file1", "parent_folder")
        .await
        .unwrap();
    assert!(matched.is_some());
    assert_eq!(matched.unwrap().id, "file1");

    // 2. Mismatched parent returns None
    let mismatched = listing
        .get_by_id_in_parent("synthetic", "file1", "parent_folder")
        .await
        .unwrap();
    assert!(mismatched.is_none());

    server.join().unwrap();
}
