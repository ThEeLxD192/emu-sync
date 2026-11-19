//! Waits of `ResumableUpload` between retries: the injected first wait and its doubling,
//! the 250 ms default, extreme values and cancellation while waiting.
//!
//! Waits are measured from the server, from the moment before it starts writing a reply to
//! the moment it accepts the next request. The client only starts waiting after reading the
//! reply, so load can lengthen a measured gap but never shorten it: lower bounds cannot fail
//! for a correct client. Upper bounds, which tell an injected wait apart from the default,
//! compare totals with a margin of hundreds of milliseconds.

use crate::common;

use common::{read_request, reply_extra as reply};
use emusync_data_drive::{ResumableUpload, UploadError, UploadSession};
use reqwest::{Client, Url};
use std::{
    io::{ErrorKind, Read},
    net::{Shutdown, TcpListener, TcpStream},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

const SNAPSHOT: &[u8] = b"abcdef";
const CREATED: &str = r#"{"id":"created","name":"save","modifiedTime":"2026-01-01T00:00:00Z"}"#;
/// First wait when `with_retry_backoff` is not called.
const DEFAULT_FIRST_WAIT: Duration = Duration::from_millis(250);
/// Leaves 200 ms for load on top of the default wait, yet rejects a first wait that is
/// already doubled (500 ms) or a longer default.
const DEFAULT_FIRST_WAIT_UPPER_BOUND: Duration = Duration::from_millis(450);

fn upload_url(listener: &TcpListener) -> Url {
    Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap()
}

fn answer_session_start(listener: &TcpListener) {
    let (mut stream, _) = listener.accept().unwrap();
    let (headers, _) = read_request(&mut stream);
    assert!(headers.starts_with("POST /upload?"), "{headers}");
    reply(&mut stream, "200 OK", "Location: /session\r\n", "");
}

/// Accepts the next request, checks that it sends the whole snapshot as one chunk and
/// returns it with the moment it was accepted.
fn accept_chunk(listener: &TcpListener) -> (TcpStream, Instant) {
    let (mut stream, _) = listener.accept().unwrap();
    let accepted_at = Instant::now();
    let (headers, body) = read_request(&mut stream);
    assert!(headers.starts_with("PUT /session"), "{headers}");
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("content-range: bytes 0-5/6"),
        "{headers}"
    );
    assert_eq!(body, SNAPSHOT);
    (stream, accepted_at)
}

/// Accepts the next request, checks that it is an empty status query and returns it with
/// the moment it was accepted.
fn accept_status_query(listener: &TcpListener) -> (TcpStream, Instant) {
    let (mut stream, _) = listener.accept().unwrap();
    let accepted_at = Instant::now();
    let (headers, body) = read_request(&mut stream);
    assert!(headers.starts_with("PUT /session"), "{headers}");
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("content-range: bytes */6"),
        "{headers}"
    );
    assert!(body.is_empty());
    (stream, accepted_at)
}

/// Replies and returns the moment before the reply started to be written, which is before
/// the client can start waiting to retry.
fn reply_noting_start(stream: &mut TcpStream, status: &str, extra: &str, body: &str) -> Instant {
    let replying_at = Instant::now();
    reply(stream, status, extra, body);
    replying_at
}

/// Returns once the client has closed the connection, that is, once it is done with the
/// reply and has moved on to what follows it.
fn wait_until_client_closes(stream: &mut TcpStream) {
    stream.shutdown(Shutdown::Write).unwrap();
    let mut byte = [0; 1];
    match stream.read(&mut byte) {
        Ok(0) => {}
        Err(error) if error.kind() == ErrorKind::ConnectionReset => {}
        other => panic!("client kept the connection open: {other:?}"),
    }
}

fn assert_no_further_request(listener: &TcpListener) {
    listener.set_nonblocking(true).unwrap();
    match listener.accept() {
        Err(error) if error.kind() == ErrorKind::WouldBlock => {}
        Err(error) => panic!("unexpected accept error: {error}"),
        Ok((_, peer)) => panic!("client sent another request from {peer}"),
    }
}

fn write_snapshot(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let path = dir.path().join("snapshot");
    std::fs::write(&path, SNAPSHOT).unwrap();
    path
}

async fn upload_snapshot(client: ResumableUpload) -> Result<String, UploadError> {
    let dir = tempfile::tempdir().unwrap();
    let snapshot = write_snapshot(&dir);
    client
        .upload(
            "synthetic",
            &snapshot,
            "save",
            "folder",
            &CancellationToken::new(),
        )
        .await
        .map(|file| file.id)
}

/// Session of an upload whose chunk response was lost and whose status query then found
/// the session expired, so its outcome is unknown. No retry wait is involved.
async fn uncertain_session(listener: &TcpListener) -> Box<UploadSession> {
    let server_listener = listener.try_clone().unwrap();
    let server = thread::spawn(move || {
        answer_session_start(&server_listener);
        let (lost_response, _) = accept_chunk(&server_listener);
        drop(lost_response);
        let (mut stream, _) = accept_status_query(&server_listener);
        reply(&mut stream, "404 Not Found", "", "");
    });
    let result = upload_snapshot(ResumableUpload::new(Client::new(), upload_url(listener))).await;
    server.join().unwrap();
    match result {
        Err(UploadError::OutcomeUnknown(session)) => session,
        other => panic!("expected an uncertain session, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn injected_backoff_spaces_reconcile_status_queries_and_doubles_between_them() {
    let first_wait = Duration::from_millis(20);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let session = uncertain_session(&listener).await;
    let server_listener = listener.try_clone().unwrap();
    let server = thread::spawn(move || {
        let mut gaps = Vec::new();
        let mut replying_at = None;
        for _ in 0..3 {
            let (mut stream, accepted_at) = accept_status_query(&server_listener);
            if let Some(replying_at) = replying_at {
                gaps.push(accepted_at.duration_since(replying_at));
            }
            replying_at = Some(reply_noting_start(
                &mut stream,
                "503 Service Unavailable",
                "",
                "",
            ));
        }
        gaps
    });

    let result = ResumableUpload::new(Client::new(), upload_url(&listener))
        .with_retry_backoff(first_wait)
        .reconcile("synthetic", &session)
        .await;

    assert!(
        matches!(result, Err(UploadError::OutcomeUnknown(_))),
        "{result:?}"
    );
    let gaps = server.join().unwrap();
    assert!(gaps[0] >= first_wait, "gaps: {gaps:?}");
    assert!(gaps[1] >= first_wait * 2, "gaps: {gaps:?}");
    // The default waits add up to 750 ms; the injected ones to 60 ms.
    assert!(
        gaps.iter().sum::<Duration>() < Duration::from_millis(500),
        "gaps: {gaps:?}"
    );
    assert_no_further_request(&listener);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn injected_backoff_spaces_unacknowledged_chunks_and_doubles_each_time() {
    let first_wait = Duration::from_millis(30);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server_listener = listener.try_clone().unwrap();
    let server = thread::spawn(move || {
        answer_session_start(&server_listener);
        let mut gaps = Vec::new();
        let mut replying_at = None;
        for _ in 0..4 {
            let (mut stream, accepted_at) = accept_chunk(&server_listener);
            if let Some(replying_at) = replying_at {
                gaps.push(accepted_at.duration_since(replying_at));
            }
            replying_at = Some(reply_noting_start(
                &mut stream,
                "308 Resume Incomplete",
                "",
                "",
            ));
        }
        gaps
    });

    let result = upload_snapshot(
        ResumableUpload::new(Client::new(), upload_url(&listener)).with_retry_backoff(first_wait),
    )
    .await;

    assert!(
        matches!(&result, Err(UploadError::RecoveryFailed { cause, .. }) if matches!(**cause, UploadError::NoProgress)),
        "{result:?}"
    );
    let gaps = server.join().unwrap();
    assert!(gaps[0] >= first_wait, "gaps: {gaps:?}");
    assert!(gaps[1] >= first_wait * 2, "gaps: {gaps:?}");
    assert!(gaps[2] >= first_wait * 4, "gaps: {gaps:?}");
    // Doubling the injected wait adds up to 210 ms; growing it fourfold, to 630 ms; the
    // default waits, to 1750 ms.
    assert!(
        gaps.iter().sum::<Duration>() < Duration::from_millis(500),
        "gaps: {gaps:?}"
    );
    assert_no_further_request(&listener);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn default_backoff_waits_250_ms_before_resending_unacknowledged_chunk() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server_listener = listener.try_clone().unwrap();
    let server = thread::spawn(move || {
        answer_session_start(&server_listener);
        let (mut stream, _) = accept_chunk(&server_listener);
        let replying_at = reply_noting_start(&mut stream, "308 Resume Incomplete", "", "");
        let (mut stream, accepted_at) = accept_chunk(&server_listener);
        reply(&mut stream, "200 OK", "", CREATED);
        accepted_at.duration_since(replying_at)
    });

    let result = upload_snapshot(ResumableUpload::new(Client::new(), upload_url(&listener))).await;

    assert_eq!(result.unwrap(), "created");
    let gap = server.join().unwrap();
    assert!(
        (DEFAULT_FIRST_WAIT..DEFAULT_FIRST_WAIT_UPPER_BOUND).contains(&gap),
        "gap: {gap:?}"
    );
    assert_no_further_request(&listener);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn default_backoff_waits_250_ms_between_status_queries() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server_listener = listener.try_clone().unwrap();
    let server = thread::spawn(move || {
        answer_session_start(&server_listener);
        let (mut stream, _) = accept_chunk(&server_listener);
        reply(&mut stream, "503 Service Unavailable", "", "");
        let (mut stream, _) = accept_status_query(&server_listener);
        let replying_at = reply_noting_start(&mut stream, "503 Service Unavailable", "", "");
        let (mut stream, accepted_at) = accept_status_query(&server_listener);
        reply(&mut stream, "200 OK", "", CREATED);
        accepted_at.duration_since(replying_at)
    });

    let result = upload_snapshot(ResumableUpload::new(Client::new(), upload_url(&listener))).await;

    assert_eq!(result.unwrap(), "created");
    let gap = server.join().unwrap();
    assert!(
        (DEFAULT_FIRST_WAIT..DEFAULT_FIRST_WAIT_UPPER_BOUND).contains(&gap),
        "gap: {gap:?}"
    );
    assert_no_further_request(&listener);
}

/// Answers the chunk and the first status query with 503, so the client starts its first
/// wait between status queries, and tells the test once the client is past the reply.
fn serve_one_failed_status_query(listener: &TcpListener, waiting_tx: &mpsc::Sender<()>) {
    answer_session_start(listener);
    let (mut stream, _) = accept_chunk(listener);
    reply(&mut stream, "503 Service Unavailable", "", "");
    let (mut stream, _) = accept_status_query(listener);
    reply(&mut stream, "503 Service Unavailable", "", "");
    wait_until_client_closes(&mut stream);
    waiting_tx.send(()).unwrap();
}

/// A century does not overflow `Instant`; `Duration::MAX` does.
const HUGE_WAITS: [Duration; 2] = [Duration::from_secs(100 * 365 * 24 * 60 * 60), Duration::MAX];

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn huge_backoff_between_status_queries_is_cancelled_with_unknown_outcome() {
    for first_wait in HUGE_WAITS {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server_listener = listener.try_clone().unwrap();
        let (waiting_tx, waiting_rx) = mpsc::channel();
        let server =
            thread::spawn(move || serve_one_failed_status_query(&server_listener, &waiting_tx));
        let dir = tempfile::tempdir().unwrap();
        let snapshot = write_snapshot(&dir);
        let client = ResumableUpload::new(Client::new(), upload_url(&listener))
            .with_retry_backoff(first_wait);
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            client
                .upload("synthetic", &snapshot, "save", "folder", &task_cancel)
                .await
        });

        waiting_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        cancel.cancel();
        let result = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("cancellation did not interrupt the wait before retrying")
            .unwrap();

        assert!(
            matches!(result, Err(UploadError::OutcomeUnknown(_))),
            "{first_wait:?}: {result:?}"
        );
        server.join().unwrap();
        assert_no_further_request(&listener);
        drop(dir);
    }
}
