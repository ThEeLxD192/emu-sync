use crate::common;

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
        Err(UpdateError::Http(status)) => {
            assert_eq!(status, reqwest::StatusCode::PARTIAL_CONTENT);
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
            Err(UpdateError::Http(status)) => {
                assert_eq!(status.as_str(), expected_code, "{status_line}");
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

#[tokio::test]
async fn refused_download_names_the_status_once_with_its_standard_reason() {
    // The download fails with the code and its standard reason, never the server's own
    // reason phrase, a repeated "HTTP" or a trailing space for a code without one.
    let cases = [
        (
            response_with_content_length("404 Not Found", b"NOT AN UPDATE"),
            "HTTP error: 404 Not Found",
        ),
        (
            response_with_content_length("503 Down for maintenance", b"NOT AN UPDATE"),
            "HTTP error: 503 Service Unavailable",
        ),
        (
            response_with_content_length("503", b""),
            "HTTP error: 503 Service Unavailable",
        ),
        (
            response_with_content_length("201 Created", b"NOT AN UPDATE"),
            "HTTP error: 201 Created",
        ),
        (
            response_with_content_length("302 Found", b""),
            "HTTP error: 302 Found",
        ),
        (
            response_with_content_length("599 Network Connect Timeout Error", b""),
            "HTTP error: 599 <unknown status code>",
        ),
        (
            response_with_content_length("599", b""),
            "HTTP error: 599 <unknown status code>",
        ),
    ];

    for (response, expected) in cases {
        let server = TestServer::bind();
        let url = server.url("/file");
        let server_task = server.spawn(move |mut connection| async move {
            connection.write_all(&response).await?;
            Ok(())
        });

        let dir = tempdir().expect("create update tempdir");
        let destination = dir.path().join("update.AppImage");
        std::fs::write(&destination, b"PREVIOUS").expect("write previous update");

        let result = tokio::time::timeout(
            common::HARNESS_WAIT_LIMIT,
            UpdateDownloader::new(client()).download(&url, &destination, |_| {}),
        )
        .await
        .unwrap_or_else(|_| panic!("{expected}: the download must end once the server closes"));

        match result {
            Err(error @ UpdateError::Http(_)) => {
                assert_eq!(error.to_string(), expected);
            }
            other => panic!("{expected}: expected an HTTP error, got: {other:?}"),
        }
        assert_eq!(
            std::fs::read(&destination).expect("read destination"),
            b"PREVIOUS",
            "{expected}"
        );
        assert_directory_entries(dir.path(), &["update.AppImage"]);
        server_task.finish().await;
    }
}

/// Downloads `content` (with its length announced) verified against `expected_sha256`,
/// and returns the result and each reported progress, with whether the destination held
/// the content when it was reported.
async fn verified_download_progress(
    content: &'static [u8],
    expected_sha256: [u8; 32],
) -> (Result<(), UpdateError>, Vec<(f32, bool)>) {
    let server = TestServer::bind();
    let url = server.url("/file");
    let server_task = server.spawn(move |mut connection| async move {
        connection
            .write_all(&response_with_content_length("200 OK", content))
            .await
    });
    let dir = tempdir().expect("create update tempdir");
    let destination = dir.path().join("update.AppImage");
    let progress = Arc::new(Mutex::new(Vec::new()));
    let progress_sink = progress.clone();
    let observed_destination = destination.clone();

    let result = UpdateDownloader::new(client())
        .download_verified(&url, &destination, expected_sha256, move |value| {
            let in_place = std::fs::read(&observed_destination).is_ok_and(|bytes| bytes == content);
            progress_sink
                .lock()
                .expect("lock progress")
                .push((value, in_place));
        })
        .await;
    server_task.finish().await;
    let progress = progress.lock().expect("lock progress").clone();
    (result, progress)
}

/// SHA-256 of `b"VERIFIED UPDATE PAYLOAD"`, from `printf 'VERIFIED UPDATE PAYLOAD' | sha256sum`.
const VERIFIED_PAYLOAD_SHA256: [u8; 32] = [
    0xc2, 0x35, 0xc5, 0x8c, 0xfc, 0xb0, 0xc8, 0x29, 0x4f, 0xb2, 0x9c, 0x31, 0xea, 0x66, 0xaa, 0xa1,
    0x40, 0xfe, 0xbb, 0x82, 0x16, 0x5e, 0x31, 0x3a, 0x79, 0x77, 0xaf, 0x7e, 0x5e, 0x1a, 0xfd, 0xc3,
];

#[tokio::test]
async fn a_verified_download_reports_completion_only_once_the_checked_file_is_in_place() {
    let (result, progress) =
        verified_download_progress(b"VERIFIED UPDATE PAYLOAD", VERIFIED_PAYLOAD_SHA256).await;

    result.expect("the payload matches its hash");
    let (last, in_place) = *progress.last().expect("progress reported");
    assert_eq!((last, in_place), (1.0, true), "{progress:?}");
    assert!(
        progress[..progress.len() - 1]
            .iter()
            .all(|(value, in_place)| *value < 1.0 && !in_place),
        "100 % before the file is checked and in place: {progress:?}"
    );
}

#[tokio::test]
async fn a_download_that_fails_verification_never_reports_completion() {
    let (result, progress) =
        verified_download_progress(b"TAMPERED UPDATE PAYLOAD", VERIFIED_PAYLOAD_SHA256).await;

    assert!(
        matches!(result, Err(UpdateError::Verification(_))),
        "{result:?}"
    );
    assert!(
        !progress.is_empty(),
        "the bytes were received: {progress:?}"
    );
    assert!(
        progress.iter().all(|(value, _)| *value < 1.0),
        "a rejected update reached 100 %: {progress:?}"
    );
}

#[tokio::test]
async fn a_download_cut_one_byte_short_never_reports_a_progress_shown_as_one_hundred_percent() {
    // 262143 of 262144 bytes: the interface shows whole percentages, rounded, so any
    // progress from 0.995 on would read «100%» for an update that is then rejected.
    const LOWEST_PROGRESS_SHOWN_AS_ONE_HUNDRED_PERCENT: f32 = 0.995;
    let announced_bytes = 256 * 1024;
    let body = vec![b'C'; announced_bytes - 1];
    let server = TestServer::bind();
    let url = server.url("/file");
    let server_task = server.spawn(move |mut connection| async move {
        let announced_length = announced_bytes.to_string();
        connection
            .write_all(&http_response(
                "200 OK",
                &[
                    ("Content-Length", announced_length.as_str()),
                    ("Connection", "close"),
                ],
                &body,
            ))
            .await
    });
    let dir = tempdir().expect("create update tempdir");
    let destination = dir.path().join("update.AppImage");
    let progress = Arc::new(Mutex::new(Vec::new()));
    let progress_sink = progress.clone();

    let result = UpdateDownloader::new(client())
        .download(&url, &destination, move |value| {
            progress_sink.lock().expect("lock progress").push(value);
        })
        .await;
    server_task.finish().await;

    assert!(
        matches!(
            result,
            Err(UpdateError::Network(_) | UpdateError::IncompleteDownload { .. })
        ),
        "{result:?}"
    );
    let progress = progress.lock().expect("lock progress").clone();
    assert!(
        !progress.is_empty(),
        "the bytes were received: {progress:?}"
    );
    assert!(
        progress
            .iter()
            .all(|value| *value < LOWEST_PROGRESS_SHOWN_AS_ONE_HUNDRED_PERCENT),
        "a rejected update read 100 %: {progress:?}"
    );
    assert_directory_entries(dir.path(), &[]);
}
