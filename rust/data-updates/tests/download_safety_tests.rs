mod common;

use common::{
    TestServer, assert_directory_entries, client, http_response, join_with_limit, recv_with_limit,
    response_with_content_length,
};
use emusync_data_updates::{UpdateDownloader, UpdateError};
use std::sync::{Arc, Mutex};
use tempfile::tempdir;
use tokio::sync::{mpsc, oneshot};

#[tokio::test]
async fn download_without_content_length_preserves_all_bytes_and_reports_completion() {
    let content = b"COMPLETE UPDATE PAYLOAD";
    let server = TestServer::bind();
    let url = server.url("/file");
    let server_task = server.spawn(move |mut connection| async move {
        // The body is delimited by connection close, not by chunked framing.
        connection
            .write_all(&http_response(
                "200 OK",
                &[("Connection", "close")],
                content,
            ))
            .await?;
        Ok(())
    });

    let dir = tempdir().expect("create update tempdir");
    let destination = dir.path().join("update.AppImage");
    let progress = Arc::new(Mutex::new(Vec::new()));
    let progress_clone = progress.clone();

    let downloader = UpdateDownloader::new(client());
    downloader
        .download(&url, &destination, move |value| {
            progress_clone.lock().expect("lock progress").push(value);
        })
        .await
        .unwrap();

    assert_eq!(
        std::fs::read(&destination).expect("read destination"),
        content
    );
    assert_eq!(*progress.lock().expect("lock progress"), vec![1.0]);
    assert_directory_entries(dir.path(), &["update.AppImage"]);

    let requests = server_task.finish().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].target, "/file");
}

#[tokio::test]
async fn download_with_content_length_reports_incremental_progress() {
    let server = TestServer::bind();
    let url = server.url("/file");
    let (release_tx, release_rx) = oneshot::channel();
    let mut server_task = server.spawn(move |mut connection| async move {
        connection
            .write_all(&http_response(
                "200 OK",
                &[("Content-Length", "8"), ("Connection", "close")],
                b"ABCD",
            ))
            .await?;
        if release_rx.await.is_err() {
            return Ok(());
        }
        connection.write_all(b"EFGH").await?;
        Ok(())
    });

    let dir = tempdir().expect("create update tempdir");
    let destination = dir.path().join("update.AppImage");
    std::fs::write(&destination, b"PREVIOUS").expect("write previous update");

    let (progress_tx, mut progress_rx) = mpsc::unbounded_channel();
    let downloader = UpdateDownloader::new(client());
    let destination_clone = destination.clone();
    let mut download_task = tokio::spawn(async move {
        downloader
            .download(&url, &destination_clone, move |value| {
                let _ = progress_tx.send(value);
            })
            .await
    });

    // TCP can split the first four bytes. Collect every valid earlier callback
    // and wait until the first half is fully reported as 0.5.
    let mut events = Vec::new();
    loop {
        let value = recv_with_limit(
            &mut progress_rx,
            &mut download_task,
            &mut server_task,
            "first body half progress",
        )
        .await;
        assert!(
            value.is_finite() && (0.0..=0.5).contains(&value),
            "progress before the second body block must stay within 0..=0.5: {value}"
        );
        if let Some(previous) = events.last() {
            assert!(
                value >= *previous,
                "progress must not decrease before the second block: {events:?} then {value}"
            );
        }
        events.push(value);
        if value == 0.5 {
            break;
        }
    }
    assert_eq!(
        *events.last().expect("progress before second block"),
        0.5,
        "first four of eight bytes must eventually report 0.5: {events:?}"
    );
    assert_eq!(
        std::fs::read(&destination).expect("read destination"),
        b"PREVIOUS",
        "destination must not change before publication"
    );
    assert!(!download_task.is_finished());

    release_tx.send(()).expect("release second body block");
    join_with_limit(
        &mut download_task,
        &mut server_task,
        "incremental progress download completion",
    )
    .await
    .expect("download succeeds");

    while let Ok(value) = progress_rx.try_recv() {
        if let Some(previous) = events.last() {
            assert!(
                value >= *previous,
                "progress must not decrease after the second block: {events:?} then {value}"
            );
        }
        events.push(value);
    }
    assert!(events.iter().all(|value| value.is_finite()));
    assert!(events.iter().all(|value| (0.0..=1.0).contains(value)));
    let first_complete = events
        .iter()
        .position(|value| *value == 1.0)
        .expect("completion progress callback");
    assert!(
        events[first_complete..].iter().all(|value| *value == 1.0),
        "only final callbacks may report 1.0: {events:?}"
    );
    assert!(
        events[..first_complete].iter().all(|value| *value < 1.0),
        "progress before completion must stay below 1.0: {events:?}"
    );
    assert_eq!(
        std::fs::read(&destination).expect("read destination"),
        b"ABCDEFGH"
    );
    assert_directory_entries(dir.path(), &["update.AppImage"]);

    server_task.finish().await;
}

#[tokio::test]
async fn empty_download_is_rejected_without_replacing_previous_file() {
    let cases = [
        (
            "content length zero",
            http_response(
                "200 OK",
                &[("Content-Length", "0"), ("Connection", "close")],
                b"",
            ),
        ),
        (
            "clean close without bytes",
            http_response("200 OK", &[("Connection", "close")], b""),
        ),
        (
            "empty chunked body",
            http_response(
                "200 OK",
                &[("Transfer-Encoding", "chunked"), ("Connection", "close")],
                b"0\r\n\r\n",
            ),
        ),
    ];

    for (name, response) in cases {
        let server = TestServer::bind();
        let url = server.url("/file");
        let server_task = server.spawn(move |mut connection| async move {
            connection.write_all(&response).await?;
            Ok(())
        });

        let dir = tempdir().expect("create update tempdir");
        let destination = dir.path().join("update.AppImage");
        std::fs::write(&destination, b"PREVIOUS").expect("write previous update");

        let progress = Arc::new(Mutex::new(Vec::new()));
        let progress_clone = progress.clone();
        let result = UpdateDownloader::new(client())
            .download(&url, &destination, move |value| {
                progress_clone.lock().expect("lock progress").push(value);
            })
            .await;

        assert!(
            matches!(result, Err(UpdateError::EmptyDownload)),
            "{name}: expected EmptyDownload, got {result:?}"
        );
        assert_eq!(
            std::fs::read(&destination).expect("read destination"),
            b"PREVIOUS",
            "{name}"
        );
        assert!(
            progress.lock().expect("lock progress").is_empty(),
            "{name}: empty downloads must not report progress"
        );
        assert_directory_entries(dir.path(), &["update.AppImage"]);
        server_task.finish().await;
    }
}

#[tokio::test]
async fn truncated_download_preserves_previous_file_and_cleans_staging() {
    let server = TestServer::bind();
    let url = server.url("/file");
    let server_task = server.spawn(move |mut connection| async move {
        connection
            .write_all(&http_response(
                "200 OK",
                &[("Content-Length", "100"), ("Connection", "close")],
                b"FOUR",
            ))
            .await?;
        Ok(())
    });

    let dir = tempdir().expect("create update tempdir");
    let destination = dir.path().join("update.AppImage");
    std::fs::write(&destination, b"PREVIOUS").expect("write previous update");

    let progress = Arc::new(Mutex::new(Vec::new()));
    let progress_clone = progress.clone();
    let result = UpdateDownloader::new(client())
        .download(&url, &destination, move |value| {
            progress_clone.lock().expect("lock progress").push(value);
        })
        .await;

    assert!(
        matches!(result, Err(UpdateError::Network(_))),
        "reqwest must report incomplete framing as Network before the defensive final check: {result:?}"
    );
    assert_eq!(
        std::fs::read(&destination).expect("read destination"),
        b"PREVIOUS"
    );
    {
        let progress = progress.lock().expect("lock progress");
        assert!(
            progress
                .iter()
                .all(|value| value.is_finite() && *value < 1.0),
            "truncated progress must stay below completion: {progress:?}"
        );
    }
    assert_directory_entries(dir.path(), &["update.AppImage"]);
    server_task.finish().await;
}

#[tokio::test]
async fn download_rejects_partial_content_206_and_preserves_destination_and_cleans_staging() {
    let server = TestServer::bind();
    let url = server.url("/file");
    let server_task = server.spawn(move |mut connection| async move {
        connection
            .write_all(&http_response(
                "206 Partial Content",
                &[
                    ("Content-Length", "4"),
                    ("Content-Range", "bytes 0-3/100"),
                    ("Connection", "close"),
                ],
                b"PART",
            ))
            .await?;
        Ok(())
    });

    let dir = tempdir().expect("create update tempdir");
    let destination = dir.path().join("update.AppImage");
    std::fs::write(&destination, b"PREVIOUS").expect("write previous update");

    let progress = Arc::new(Mutex::new(Vec::new()));
    let progress_clone = progress.clone();
    let result = UpdateDownloader::new(client())
        .download(&url, &destination, move |value| {
            progress_clone.lock().expect("lock progress").push(value);
        })
        .await;

    match result {
        Err(UpdateError::Http(message)) => {
            assert!(message.contains("206"), "expected 206 in error: {message}");
        }
        other => panic!("expected HTTP 206 error, got: {other:?}"),
    }
    assert_eq!(
        std::fs::read(&destination).expect("read destination"),
        b"PREVIOUS"
    );
    assert!(progress.lock().expect("lock progress").is_empty());
    assert_directory_entries(dir.path(), &["update.AppImage"]);
    server_task.finish().await;
}

#[tokio::test]
async fn rejected_http_statuses_never_publish_response_bodies() {
    let cases = [
        (
            "204 No Content",
            http_response("204 No Content", &[("Connection", "close")], b""),
            "204",
        ),
        (
            "404 Not Found",
            response_with_content_length("404 Not Found", b"NOT AN UPDATE"),
            "404",
        ),
        (
            "429 Too Many Requests",
            response_with_content_length("429 Too Many Requests", b"NOT AN UPDATE"),
            "429",
        ),
        (
            "500 Internal Server Error",
            response_with_content_length("500 Internal Server Error", b"NOT AN UPDATE"),
            "500",
        ),
        (
            "503 Service Unavailable",
            response_with_content_length("503 Service Unavailable", b"NOT AN UPDATE"),
            "503",
        ),
    ];

    for (status_line, response, expected_code) in cases {
        let server = TestServer::bind();
        let url = server.url("/file");
        let server_task = server.spawn(move |mut connection| async move {
            connection.write_all(&response).await?;
            Ok(())
        });

        let dir = tempdir().expect("create update tempdir");
        let destination = dir.path().join("update.AppImage");
        std::fs::write(&destination, b"PREVIOUS").expect("write previous update");

        let progress = Arc::new(Mutex::new(Vec::new()));
        let progress_clone = progress.clone();
        let result = UpdateDownloader::new(client())
            .download(&url, &destination, move |value| {
                progress_clone.lock().expect("lock progress").push(value);
            })
            .await;

        match result {
            Err(UpdateError::Http(message)) => {
                assert!(
                    message.contains(expected_code),
                    "{status_line}: expected {expected_code} in error: {message}"
                );
            }
            other => panic!("{status_line}: expected HTTP error, got: {other:?}"),
        }
        assert_eq!(
            std::fs::read(&destination).expect("read destination"),
            b"PREVIOUS",
            "{status_line}"
        );
        assert!(
            progress.lock().expect("lock progress").is_empty(),
            "{status_line}: rejected responses must not report progress"
        );
        assert_directory_entries(dir.path(), &["update.AppImage"]);
        server_task.finish().await;
    }
}

#[tokio::test]
async fn chunked_download_requires_valid_complete_framing() {
    let valid = http_response(
        "200 OK",
        &[("Transfer-Encoding", "chunked"), ("Connection", "close")],
        b"4\r\nABCD\r\n4\r\nEFGH\r\n0\r\n\r\n",
    );
    let invalid = [
        (
            "truncated inside a chunk",
            http_response(
                "200 OK",
                &[("Transfer-Encoding", "chunked"), ("Connection", "close")],
                b"8\r\nABCD",
            ),
        ),
        (
            "invalid chunk size",
            http_response(
                "200 OK",
                &[("Transfer-Encoding", "chunked"), ("Connection", "close")],
                b"Z\r\nABCD\r\n",
            ),
        ),
        (
            "missing final chunk",
            http_response(
                "200 OK",
                &[("Transfer-Encoding", "chunked"), ("Connection", "close")],
                b"4\r\nABCD\r\n",
            ),
        ),
    ];

    let server = TestServer::bind();
    let url = server.url("/file");
    let server_task = server.spawn(move |mut connection| async move {
        connection.write_all(&valid).await?;
        Ok(())
    });
    let dir = tempdir().expect("create update tempdir");
    let destination = dir.path().join("update.AppImage");
    std::fs::write(&destination, b"PREVIOUS").expect("write previous update");
    let progress = Arc::new(Mutex::new(Vec::new()));
    let progress_clone = progress.clone();
    let result = UpdateDownloader::new(client())
        .download(&url, &destination, move |value| {
            progress_clone.lock().expect("lock progress").push(value);
        })
        .await;

    result.expect("valid chunked download");
    assert_eq!(
        std::fs::read(&destination).expect("read destination"),
        b"ABCDEFGH"
    );
    assert_eq!(*progress.lock().expect("lock progress"), vec![1.0]);
    assert_directory_entries(dir.path(), &["update.AppImage"]);
    server_task.finish().await;

    for (name, response) in invalid {
        let server = TestServer::bind();
        let url = server.url("/file");
        let server_task = server.spawn(move |mut connection| async move {
            connection.write_all(&response).await?;
            Ok(())
        });

        let dir = tempdir().expect("create update tempdir");
        let destination = dir.path().join("update.AppImage");
        std::fs::write(&destination, b"PREVIOUS").expect("write previous update");
        let progress = Arc::new(Mutex::new(Vec::new()));
        let progress_clone = progress.clone();
        let result = UpdateDownloader::new(client())
            .download(&url, &destination, move |value| {
                progress_clone.lock().expect("lock progress").push(value);
            })
            .await;

        assert!(
            matches!(result, Err(UpdateError::Network(_))),
            "{name}: malformed chunked framing must be Network, got {result:?}"
        );
        assert_eq!(
            std::fs::read(&destination).expect("read destination"),
            b"PREVIOUS",
            "{name}"
        );
        assert!(
            progress.lock().expect("lock progress").is_empty(),
            "{name}: unknown total length must not report progress"
        );
        assert_directory_entries(dir.path(), &["update.AppImage"]);
        server_task.finish().await;
    }
}

#[tokio::test]
async fn invalid_destination_parent_fails_before_http_and_preserves_files() {
    let dir = tempdir().expect("create update tempdir");
    let parent = dir.path().join("parent");
    std::fs::write(&parent, b"BLOCKER").expect("write blocking parent file");
    let destination = parent.join("update.AppImage");

    let server = TestServer::bind();
    let url = server.url("/file");
    let progress = Arc::new(Mutex::new(Vec::new()));
    let progress_clone = progress.clone();
    let result = UpdateDownloader::new(client())
        .download(&url, &destination, move |value| {
            progress_clone.lock().expect("lock progress").push(value);
        })
        .await;

    assert!(
        matches!(result, Err(UpdateError::Io(_))),
        "expected create_dir_all I/O failure, got {result:?}"
    );
    assert_eq!(
        std::fs::read(&parent).expect("read blocking parent"),
        b"BLOCKER"
    );
    assert!(progress.lock().expect("lock progress").is_empty());
    assert_directory_entries(dir.path(), &["parent"]);
    server.assert_no_connection();
}
