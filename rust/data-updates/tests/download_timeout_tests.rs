mod common;

use common::{
    ServerTask, TestServer, assert_directory_entries, client, directory_entries, http_response,
    join_with_limit, part_entries, recv_with_limit,
};
use emusync_data_updates::downloader::DEFAULT_INACTIVITY_TIMEOUT;
use emusync_data_updates::{UpdateDownloader, UpdateError};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::tempdir;
use tokio::sync::{mpsc, oneshot};

/// Kotlin `UPDATE_SOCKET_TIMEOUT_MS = 60_000L`. Keep this literal independent
/// from the production constant so a drifted default cannot silently extend the
/// test's virtual-time advance.
const KOTLIN_SOCKET_TIMEOUT: Duration = Duration::from_secs(60);

fn assert_default_timeout_matches_kotlin() {
    assert_eq!(
        DEFAULT_INACTIVITY_TIMEOUT, KOTLIN_SOCKET_TIMEOUT,
        "Kotlin UPDATE_SOCKET_TIMEOUT_MS is 60_000L; production default drifted"
    );
}

async fn recv_progress_at_least(
    receiver: &mut mpsc::UnboundedReceiver<f32>,
    target: f32,
    events: &mut Vec<f32>,
    download_task: &mut tokio::task::JoinHandle<Result<(), UpdateError>>,
    server_task: &mut ServerTask,
    context: &str,
) -> f32 {
    loop {
        let value = recv_with_limit(receiver, download_task, server_task, context).await;
        assert!(
            value.is_finite() && (0.0..=1.0).contains(&value),
            "{context}: progress out of range: {value}"
        );
        if let Some(previous) = events.last() {
            assert!(
                value >= *previous,
                "{context}: progress must not decrease: {events:?} then {value}"
            );
        }
        events.push(value);
        if value >= target {
            return value;
        }
    }
}

#[tokio::test]
async fn inactivity_timeout_aborts_when_server_does_not_deliver_headers() {
    assert_default_timeout_matches_kotlin();
    let server = TestServer::bind();
    let url = server.url("/file");
    let (release_tx, release_rx) = oneshot::channel::<()>();
    let mut server_task = server.spawn(move |connection| async move {
        // Keep the socket open without sending any response headers.
        let _connection = connection;
        let _ = release_rx.await;
        Ok(())
    });
    let dir = tempdir().expect("create update tempdir");
    let destination = dir.path().join("update.AppImage");
    std::fs::write(&destination, b"PREVIOUS").expect("write previous update");

    let progress = Arc::new(Mutex::new(Vec::new()));
    let progress_clone = progress.clone();
    let downloader = UpdateDownloader::new(client());
    let destination_clone = destination.clone();
    let mut download_task = tokio::spawn(async move {
        downloader
            .download(&url, &destination_clone, move |value| {
                progress_clone.lock().expect("lock progress").push(value);
            })
            .await
    });

    let request = server_task
        .wait_for_request(&mut download_task, "header request")
        .await;
    assert_eq!(request.method, "GET");
    assert_eq!(request.target, "/file");

    tokio::time::pause();
    tokio::time::advance(KOTLIN_SOCKET_TIMEOUT + Duration::from_secs(1)).await;
    tokio::time::resume();

    let result = join_with_limit(
        &mut download_task,
        &mut server_task,
        "header inactivity timeout",
    )
    .await;
    match result {
        Err(UpdateError::Network(message)) => {
            assert!(
                message.contains("timed out"),
                "expected request timeout message, got: {message}"
            );
        }
        other => panic!("expected Network timeout before headers, got {other:?}"),
    }

    drop(release_tx);
    let requests = server_task.finish().await;
    assert_eq!(requests.len(), 1);

    assert_eq!(
        std::fs::read(&destination).expect("read destination"),
        b"PREVIOUS"
    );
    assert!(progress.lock().expect("lock progress").is_empty());
    assert_directory_entries(dir.path(), &["update.AppImage"]);
}

#[tokio::test]
async fn inactivity_timeout_aborts_when_server_delivers_headers_then_stalls_body() {
    assert_default_timeout_matches_kotlin();
    let server = TestServer::bind();
    let url = server.url("/file");
    let (release_tx, release_rx) = oneshot::channel::<()>();
    let mut server_task = server.spawn(move |mut connection| async move {
        connection
            .write_all(&http_response(
                "200 OK",
                &[("Content-Length", "8"), ("Connection", "keep-alive")],
                b"ABCD",
            ))
            .await?;
        let _ = release_rx.await;
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

    let mut first_events = Vec::new();
    let first = recv_progress_at_least(
        &mut progress_rx,
        0.5,
        &mut first_events,
        &mut download_task,
        &mut server_task,
        "stalled body first half",
    )
    .await;
    assert_eq!(first, 0.5);
    assert!(
        first_events.iter().all(|value| *value <= 0.5),
        "first body half cannot exceed 0.5: {first_events:?}"
    );
    assert!(
        !download_task.is_finished(),
        "download must remain pending while the body stalls"
    );
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
    assert_eq!(part_entries(dir.path()).len(), 1);

    tokio::time::pause();
    tokio::time::advance(KOTLIN_SOCKET_TIMEOUT + Duration::from_secs(1)).await;
    tokio::time::resume();

    let result = join_with_limit(
        &mut download_task,
        &mut server_task,
        "stalled body inactivity timeout",
    )
    .await;
    match result {
        Err(UpdateError::Network(message)) => {
            assert!(
                message.contains("download stalled"),
                "expected body inactivity message, got: {message}"
            );
        }
        other => panic!("expected Network stalled-body error, got {other:?}"),
    }

    drop(release_tx);
    server_task.finish().await;

    assert_eq!(
        std::fs::read(&destination).expect("read destination"),
        b"PREVIOUS"
    );
    assert_directory_entries(dir.path(), &["update.AppImage"]);
    assert!(part_entries(dir.path()).is_empty());
}

#[tokio::test]
async fn active_transfer_with_periodic_progress_survives_past_watchdog_duration() {
    assert_default_timeout_matches_kotlin();
    let chunk1: &'static [u8] = b"0123456789";
    let chunk2: &'static [u8] = b"ABCDEFGHIJ";
    let chunk3: &'static [u8] = b"klmnopqrst";
    let expected_body = b"0123456789ABCDEFGHIJklmnopqrst";

    let server = TestServer::bind();
    let url = server.url("/file");
    let (chunk2_tx, chunk2_rx) = oneshot::channel();
    let (chunk3_tx, chunk3_rx) = oneshot::channel();
    let (close_tx, close_rx) = oneshot::channel();
    let mut server_task = server.spawn(move |mut connection| async move {
        connection
            .write_all(&http_response(
                "200 OK",
                &[("Content-Length", "30"), ("Connection", "close")],
                chunk1,
            ))
            .await?;
        if chunk2_rx.await.is_err() {
            return Ok(());
        }
        connection.write_all(chunk2).await?;
        if chunk3_rx.await.is_err() {
            return Ok(());
        }
        connection.write_all(chunk3).await?;
        let _ = close_rx.await;
        Ok(())
    });

    let dir = tempdir().expect("create update tempdir");
    let destination = dir.path().join("update.AppImage");

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

    let mut events = Vec::new();
    let p1 = recv_progress_at_least(
        &mut progress_rx,
        10.0 / 30.0,
        &mut events,
        &mut download_task,
        &mut server_task,
        "first chunk progress",
    )
    .await;
    assert!(
        (p1 - (10.0_f32 / 30.0)).abs() < 1e-6,
        "first chunk progress expected 10/30, got {p1}"
    );

    tokio::time::pause();
    let start_virtual = tokio::time::Instant::now();
    tokio::time::advance(Duration::from_secs(35)).await;
    tokio::time::resume();
    chunk2_tx.send(()).expect("release second chunk");
    let p2 = recv_progress_at_least(
        &mut progress_rx,
        20.0 / 30.0,
        &mut events,
        &mut download_task,
        &mut server_task,
        "second chunk progress",
    )
    .await;
    assert!(
        (p2 - (20.0_f32 / 30.0)).abs() < 1e-6,
        "second chunk progress expected 20/30, got {p2}"
    );

    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(35)).await;
    tokio::time::resume();
    chunk3_tx.send(()).expect("release third chunk");
    let p3 = recv_progress_at_least(
        &mut progress_rx,
        1.0,
        &mut events,
        &mut download_task,
        &mut server_task,
        "third chunk progress",
    )
    .await;
    assert!(
        (p3 - 1.0).abs() < f32::EPSILON,
        "third chunk progress expected 30/30, got {p3}"
    );

    close_tx.send(()).expect("close transfer");
    join_with_limit(
        &mut download_task,
        &mut server_task,
        "active transfer completion",
    )
    .await
    .expect("download succeeds");

    while let Ok(value) = progress_rx.try_recv() {
        if let Some(previous) = events.last() {
            assert!(
                value >= *previous,
                "progress must not decrease after completion: {events:?} then {value}"
            );
        }
        events.push(value);
    }
    assert!(
        events.len() >= 3,
        "expected progress observations for all three chunks: {events:?}"
    );
    assert!(events.iter().all(|value| value.is_finite()));
    assert!(events.iter().all(|value| (0.0..=1.0).contains(value)));
    assert!(
        events.windows(2).all(|pair| pair[0] <= pair[1]),
        "progress must be non-decreasing: {events:?}"
    );
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

    tokio::time::pause();
    let elapsed_virtual = start_virtual.elapsed();
    assert!(
        elapsed_virtual >= Duration::from_secs(70),
        "total virtual transfer time ({elapsed_virtual:?}) must exceed the watchdog"
    );
    tokio::time::resume();

    let requests = server_task.finish().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        std::fs::read(&destination).expect("read destination"),
        expected_body
    );
    assert_directory_entries(dir.path(), &["update.AppImage"]);
    assert!(part_entries(dir.path()).is_empty());
}
