//! How long a conditional PUT already in flight may still finish after cancellation
//! (`ConditionalUpdate::with_cancel_grace`), in both the wait for the response headers and
//! the read of the response metadata.

use crate::common;

use common::{read_request, reply};
use emusync_data_drive::{ConditionalError, ConditionalUpdate, FileRevision, UploadedFile};
use reqwest::{Client, Url};
use std::{
    io::Write,
    net::TcpListener,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

const SMALL_GRACE: Duration = Duration::from_millis(50);
/// The default grace is one second; a small injected grace must end well before it.
const WELL_BEFORE_DEFAULT_GRACE: Duration = Duration::from_millis(800);

fn metadata_body(id: &str, etag: &str) -> String {
    format!(
        r#"{{"id":"{id}","title":"save.dat","modifiedDate":"2026-01-01T00:00:00Z","version":"1","etag":"\"{etag}\""}}"#
    )
}

fn observed_revision() -> FileRevision {
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

/// Response headers and the first byte of `body`, with the rest of the body still owed.
fn headers_and_first_body_byte(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        &body[..1]
    )
}

/// What the fake Drive writes on the PUT connection before and after the test cancels.
struct PutResponse {
    before_cancel: String,
    /// `None` keeps the connection silent until the client has returned.
    after_cancel: Option<LateResponse>,
}

/// The rest of the response, written once the cancelled update has kept waiting for
/// `held_back_for` without returning.
struct LateResponse {
    bytes: String,
    held_back_for: Duration,
}

struct CancelledPut {
    result: Result<UploadedFile, ConditionalError>,
    cancel_to_result: Duration,
}

/// Runs a conditional update of `abc`, cancels it once the fake Drive has read the whole
/// PUT body and written `before_cancel`, and then lets the fake Drive write
/// `after_cancel`. When a late response exists, first checks that the cancelled update
/// keeps waiting for its `held_back_for` instead of returning.
async fn cancel_during_conditional_put(
    client: ConditionalUpdate,
    listener: TcpListener,
    response: PutResponse,
) -> CancelledPut {
    let (put_read_tx, put_read_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let held_back_for = response
        .after_cancel
        .as_ref()
        .map(|late_response| late_response.held_back_for);
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, _) = read_request(&mut stream);
        assert!(headers.starts_with("GET /files/file?"));
        reply(&mut stream, "200 OK", &metadata_body("file", "v1"));
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut stream);
        assert!(headers.starts_with("PUT /upload/file?"));
        assert_eq!(body, b"abc");
        stream.write_all(response.before_cancel.as_bytes()).unwrap();
        stream.flush().unwrap();
        put_read_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        if let Some(late_response) = response.after_cancel {
            stream.write_all(late_response.bytes.as_bytes()).unwrap();
            stream.flush().unwrap();
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let snapshot_path = dir.path().join("snapshot");
    std::fs::write(&snapshot_path, b"abc").unwrap();
    let cancel = CancellationToken::new();
    let worker_cancel = cancel.clone();
    let mut task = tokio::spawn(async move {
        client
            .update_with_cancel(
                "synthetic",
                &observed_revision(),
                &snapshot_path,
                "save.dat",
                Arc::new(|_| {}),
                &worker_cancel,
            )
            .await
    });
    put_read_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let cancelled_at = Instant::now();
    cancel.cancel();
    if let Some(held_back_for) = held_back_for {
        assert!(
            tokio::time::timeout(held_back_for, &mut task)
                .await
                .is_err(),
            "the cancelled update returned before its in-flight response arrived"
        );
        release_tx.send(()).unwrap();
    }
    let result = tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .expect("the cancelled update never returned")
        .expect("the cancelled update panicked");
    let cancel_to_result = cancelled_at.elapsed();
    if held_back_for.is_none() {
        release_tx.send(()).unwrap();
    }
    server.join().unwrap();
    CancelledPut {
        result,
        cancel_to_result,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn small_cancel_grace_reports_unknown_outcome_well_before_a_second_when_put_response_is_silent()
 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = adapter(&listener).with_cancel_grace(SMALL_GRACE);
    let cancelled = cancel_during_conditional_put(
        client,
        listener,
        PutResponse {
            before_cancel: String::new(),
            after_cancel: None,
        },
    )
    .await;
    assert!(
        matches!(
            cancelled.result,
            Err(ConditionalError::CancelledOutcomeUnknown)
        ),
        "{:?}",
        cancelled.result
    );
    assert!(
        (SMALL_GRACE..WELL_BEFORE_DEFAULT_GRACE).contains(&cancelled.cancel_to_result),
        "the 50 ms grace took {:?}",
        cancelled.cancel_to_result
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn small_cancel_grace_reports_unknown_outcome_well_before_a_second_when_success_metadata_stalls()
 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = adapter(&listener).with_cancel_grace(SMALL_GRACE);
    let cancelled = cancel_during_conditional_put(
        client,
        listener,
        PutResponse {
            before_cancel: headers_and_first_body_byte(&metadata_body("file", "v2")),
            after_cancel: None,
        },
    )
    .await;
    assert!(
        matches!(
            cancelled.result,
            Err(ConditionalError::CancelledOutcomeUnknown)
        ),
        "{:?}",
        cancelled.result
    );
    assert!(
        (SMALL_GRACE..WELL_BEFORE_DEFAULT_GRACE).contains(&cancelled.cancel_to_result),
        "the 50 ms grace took {:?}",
        cancelled.cancel_to_result
    );
}

/// `Duration::MAX` must not overflow the grace timer; the late body arrives well inside
/// even the default grace, so this does not show that a grace above one second is kept.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn maximum_cancel_grace_does_not_overflow_and_completes_metadata_resumed_after_cancellation()
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = adapter(&listener).with_cancel_grace(Duration::MAX);
    let body = metadata_body("file", "v2");
    let cancelled = cancel_during_conditional_put(
        client,
        listener,
        PutResponse {
            before_cancel: headers_and_first_body_byte(&body),
            after_cancel: Some(LateResponse {
                bytes: body[1..].to_owned(),
                held_back_for: Duration::from_millis(100),
            }),
        },
    )
    .await;
    let uploaded = cancelled.result.unwrap();
    assert_eq!(uploaded.id, "file");
    assert_eq!(uploaded.title, "save.dat");
}

/// Without `with_cancel_grace` the grace stays one second: a silent PUT neither gives up
/// sooner nor keeps a cancelled sync waiting much longer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn default_cancel_grace_reports_unknown_outcome_between_one_and_three_seconds_after_cancellation()
 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = adapter(&listener);
    let cancelled = cancel_during_conditional_put(
        client,
        listener,
        PutResponse {
            before_cancel: String::new(),
            after_cancel: None,
        },
    )
    .await;
    assert!(
        matches!(
            cancelled.result,
            Err(ConditionalError::CancelledOutcomeUnknown)
        ),
        "{:?}",
        cancelled.result
    );
    assert!(
        (Duration::from_secs(1)..Duration::from_secs(3)).contains(&cancelled.cancel_to_result),
        "the default grace took {:?}",
        cancelled.cancel_to_result
    );
}
