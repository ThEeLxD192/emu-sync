mod common;

use common::{
    TestServer, assert_directory_entries, client, directory_entries, http_response, part_entries,
    recv_with_limit,
};
use emusync_data_updates::UpdateDownloader;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;
use tokio::sync::{mpsc, oneshot};

#[tokio::test]
async fn cancelled_download_preserves_destination_and_cleans_staging() {
    let server = TestServer::bind();
    let url = server.url("/file");
    let (keep_open_tx, keep_open_rx) = oneshot::channel::<()>();
    let mut server_task = server.spawn(move |mut connection| async move {
        connection
            .write_all(&http_response(
                "200 OK",
                &[("Content-Length", "8"), ("Connection", "keep-alive")],
                b"ABCD",
            ))
            .await?;
        let _ = keep_open_rx.await;
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

    // The 0.5 callback proves headers, staging and the first write were processed
    // while more body bytes are still outstanding. TCP may fragment the first
    // four bytes, so accept valid earlier callbacks and wait for 0.5.
    let mut first_events = Vec::new();
    loop {
        let value = recv_with_limit(
            &mut progress_rx,
            &mut download_task,
            &mut server_task,
            "cancelled download first half progress",
        )
        .await;
        assert!(
            value.is_finite() && (0.0..=0.5).contains(&value),
            "progress before the second body block must stay within 0..=0.5: {value}"
        );
        if let Some(previous) = first_events.last() {
            assert!(
                value >= *previous,
                "progress must not decrease before the second block: {first_events:?} then {value}"
            );
        }
        first_events.push(value);
        if value == 0.5 {
            break;
        }
    }
    let first = *first_events.last().expect("first half progress");
    assert_eq!(first, 0.5);
    assert_eq!(
        std::fs::read(&destination).expect("read destination"),
        b"PREVIOUS"
    );
    let entries = directory_entries(dir.path());
    assert_eq!(
        entries.len(),
        2,
        "expected destination plus one staging file"
    );
    assert_eq!(entries[1], "update.AppImage");
    assert!(entries[0].starts_with(".update.AppImage."));
    assert!(entries[0].ends_with(".part"));
    assert_eq!(part_entries(dir.path()).len(), 1);
    assert!(!download_task.is_finished());

    download_task.abort();
    let join_error = download_task
        .await
        .expect_err("aborted download task must not return a result");
    assert!(
        join_error.is_cancelled(),
        "expected cancellation, got {join_error:?}"
    );

    drop(keep_open_tx);
    server_task.finish().await;

    assert_eq!(
        std::fs::read(&destination).expect("read destination"),
        b"PREVIOUS"
    );
    assert_directory_entries(dir.path(), &["update.AppImage"]);
    assert!(part_entries(dir.path()).is_empty());
}

#[tokio::test]
async fn publication_failure_preserves_destination_and_removes_staging() {
    let server = TestServer::bind();
    let url = server.url("/file");
    let server_task = server.spawn(move |mut connection| async move {
        connection
            .write_all(&http_response(
                "200 OK",
                &[("Connection", "close")],
                b"UPDATE",
            ))
            .await?;
        Ok(())
    });

    let dir = tempdir().expect("create update tempdir");
    let destination = dir.path().join("update.AppImage");
    std::fs::create_dir(&destination).expect("create destination directory");
    std::fs::write(destination.join("marker"), b"PREVIOUS").expect("write destination marker");

    let progress = Arc::new(Mutex::new(Vec::new()));
    let progress_clone = progress.clone();
    let result = UpdateDownloader::new(client())
        .download(&url, &destination, move |value| {
            progress_clone.lock().expect("lock progress").push(value);
        })
        .await;

    assert!(
        matches!(result, Err(emusync_data_updates::UpdateError::Io(_))),
        "renaming a file over a non-empty directory must fail with Io: {result:?}"
    );
    assert_eq!(
        std::fs::read(destination.join("marker")).expect("read destination marker"),
        b"PREVIOUS"
    );
    assert!(
        progress.lock().expect("lock progress").is_empty(),
        "without Content-Length the only callback is after publication"
    );
    assert_directory_entries(dir.path(), &["update.AppImage"]);
    assert!(part_entries(dir.path()).is_empty());
    server_task.finish().await;
}

#[tokio::test]
async fn progress_callback_panic_before_publication_preserves_previous_file() {
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
        let _ = release_rx.await;
        Ok(())
    });

    let dir = tempdir().expect("create update tempdir");
    let destination = dir.path().join("update.AppImage");
    std::fs::write(&destination, b"PREVIOUS").expect("write previous update");

    let downloader = UpdateDownloader::new(client());
    let destination_clone = destination.clone();
    let mut download_task = tokio::spawn(async move {
        downloader
            .download(&url, &destination_clone, |value| {
                if value == 0.5 {
                    panic!("progress callback panic");
                }
            })
            .await
    });

    let join_error =
        match tokio::time::timeout(common::HARNESS_WAIT_LIMIT, &mut download_task).await {
            Ok(Ok(Ok(()))) => panic!("panicking callback must not produce a successful result"),
            Ok(Ok(Err(error))) => {
                drop(release_tx);
                server_task.finish().await;
                panic!("expected callback panic, got download error: {error:?}");
            }
            Ok(Err(error)) => error,
            Err(_) => {
                download_task.abort();
                let _ = download_task.await;
                drop(release_tx);
                server_task.abort().await;
                panic!(
                    "callback panic was not observed within harness wait limit {:?}",
                    common::HARNESS_WAIT_LIMIT
                );
            }
        };
    assert!(join_error.is_panic(), "expected panic, got {join_error:?}");
    let payload = join_error.into_panic();
    if let Some(message) = payload.downcast_ref::<&str>() {
        assert_eq!(*message, "progress callback panic");
    } else if let Some(message) = payload.downcast_ref::<String>() {
        assert_eq!(message, "progress callback panic");
    } else {
        panic!("unexpected panic payload type");
    }

    release_tx.send(()).expect("release stalled server");
    server_task.finish().await;

    assert_eq!(
        std::fs::read(&destination).expect("read destination"),
        b"PREVIOUS"
    );
    assert_directory_entries(dir.path(), &["update.AppImage"]);
    assert!(part_entries(dir.path()).is_empty());
}

#[tokio::test]
async fn cancelling_one_download_does_not_remove_another_download_or_foreign_parts() {
    let dir = tempdir().expect("create update tempdir");
    let destination_a = dir.path().join("a.AppImage");
    let destination_b = dir.path().join("b.AppImage");
    let foreign_part = dir.path().join("foreign.part");
    std::fs::write(&destination_a, b"PREVIOUS-A").expect("write previous A");
    std::fs::write(&destination_b, b"PREVIOUS-B").expect("write previous B");
    std::fs::write(&foreign_part, b"FOREIGN").expect("write foreign part");

    let server_a = TestServer::bind();
    let url_a = server_a.url("/a");
    let (release_a_tx, release_a_rx) = oneshot::channel::<()>();
    let mut server_a_task = server_a.spawn(move |mut connection| async move {
        connection
            .write_all(&http_response(
                "200 OK",
                &[("Content-Length", "8"), ("Connection", "keep-alive")],
                b"AAAA",
            ))
            .await?;
        let _ = release_a_rx.await;
        Ok(())
    });

    let server_b = TestServer::bind();
    let url_b = server_b.url("/b");
    let server_b_task = server_b.spawn(move |mut connection| async move {
        connection
            .write_all(&http_response(
                "200 OK",
                &[("Content-Length", "8"), ("Connection", "close")],
                b"BBBBBBBB",
            ))
            .await?;
        Ok(())
    });

    let downloader = UpdateDownloader::new(client());
    let (progress_tx, mut progress_rx) = mpsc::unbounded_channel();
    let downloader_a = downloader.clone();
    let destination_a_clone = destination_a.clone();
    let mut task_a = tokio::spawn(async move {
        downloader_a
            .download(&url_a, &destination_a_clone, move |value| {
                let _ = progress_tx.send(value);
            })
            .await
    });

    let mut first_events = Vec::new();
    loop {
        let value = recv_with_limit(
            &mut progress_rx,
            &mut task_a,
            &mut server_a_task,
            "download A first half progress",
        )
        .await;
        assert!(
            value.is_finite() && (0.0..=0.5).contains(&value),
            "download A progress before the second block must stay within 0..=0.5: {value}"
        );
        if let Some(previous) = first_events.last() {
            assert!(
                value >= *previous,
                "download A progress must not decrease: {first_events:?} then {value}"
            );
        }
        first_events.push(value);
        if value == 0.5 {
            break;
        }
    }
    let first = *first_events.last().expect("download A first half progress");
    assert_eq!(first, 0.5);
    assert!(!task_a.is_finished());

    downloader
        .download(&url_b, &destination_b, |_| {})
        .await
        .expect("download B completes");

    task_a.abort();
    let join_error = task_a
        .await
        .expect_err("aborted download A must not return a result");
    assert!(
        join_error.is_cancelled(),
        "expected cancellation, got {join_error:?}"
    );

    drop(release_a_tx);
    server_a_task.finish().await;
    server_b_task.finish().await;

    assert_eq!(
        std::fs::read(&destination_a).expect("read destination A"),
        b"PREVIOUS-A"
    );
    assert_eq!(
        std::fs::read(&destination_b).expect("read destination B"),
        b"BBBBBBBB"
    );
    assert_eq!(
        std::fs::read(&foreign_part).expect("read foreign part"),
        b"FOREIGN"
    );
    assert_directory_entries(dir.path(), &["a.AppImage", "b.AppImage", "foreign.part"]);
    assert_eq!(part_entries(dir.path()), vec!["foreign.part"]);
}
