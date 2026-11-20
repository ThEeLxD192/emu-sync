//! Transport policy of the release client: plain HTTP is refused before any connection
//! when TLS is required, a controlled server works over plain HTTP (redirects included),
//! a release check cannot hang, and a progressing download has no overall limit.

use crate::common;

use common::{
    TestServer, assert_directory_entries, http_response, join_with_limit, part_entries,
    recv_with_limit, response_with_content_length,
    signed_release::{
        SIGNED_MANIFEST_URL, SIGNED_SIGNATURE_URL, SignedFilesServer, signed_manifest,
        test_release_key,
    },
};
use emusync_data_updates::{
    GitHubUpdateSource, ReleaseTransport, UpdateDownloader, UpdateError, release_client_builder,
};
use emusync_domain::update::{UpdateInfo, UpdateSource};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::tempdir;
use tokio::sync::{mpsc, oneshot};

const RELEASE_ENDPOINT: &str = "/repos/owner/repo/releases/latest";

/// The specified longest release check.
const SPECIFIED_CHECK_LIMIT: Duration = Duration::from_secs(60);

/// The specified inactivity limit of `UpdateDownloader` (Kotlin `UPDATE_SOCKET_TIMEOUT_MS`).
const SPECIFIED_INACTIVITY_LIMIT: Duration = Duration::from_secs(60);

/// The cause `reqwest` gives when a policy refuses a URL scheme. Without it, the message
/// only says that building the request failed.
const REFUSED_SCHEME_CAUSE: &str = "URL scheme is not allowed";

/// The cause `reqwest` gives when a request outlives its timeout. Without it, the message
/// only says that sending the request or reading its body failed.
const CHECK_TIMEOUT_CAUSE: &str = "operation timed out";

const RELEASE_BODY: &str = r#"{"tag_name":"v2.0.0","name":"EmuSync 2.0.0","body":"Fixes","assets":[{"name":"EmuSync-SHA256SUMS","browser_download_url":"https://example.com/download/EmuSync-SHA256SUMS","size":200},{"name":"EmuSync-SHA256SUMS.sig","browser_download_url":"https://example.com/download/EmuSync-SHA256SUMS.sig","size":89},{"name":"EmuSync-x86_64.AppImage","browser_download_url":"https://example.com/download/EmuSync-x86_64.AppImage","size":1024}]}"#;

const ASSET_BYTES: &[u8] = b"new EmuSync AppImage bytes";

/// The production release policy, kept away from any proxy set in the environment so
/// requests reach the local test server.
fn release_client_without_proxy(transport: ReleaseTransport) -> reqwest::Client {
    release_client_builder(transport)
        .no_proxy()
        .build()
        .expect("build release client")
}

fn https_only_client() -> reqwest::Client {
    release_client_without_proxy(ReleaseTransport::HttpsOnly)
}

fn plain_http_client() -> reqwest::Client {
    release_client_without_proxy(ReleaseTransport::AllowPlainHttp)
}

fn expected_release_update() -> UpdateInfo {
    UpdateInfo {
        version: "2.0.0".into(),
        tag_name: "v2.0.0".into(),
        release_notes: "Fixes".into(),
        download_url: "https://example.com/download/EmuSync-x86_64.AppImage".into(),
        asset_size: 1024,
        asset_name: "EmuSync-x86_64.AppImage".into(),
        manifest_url: SIGNED_MANIFEST_URL.into(),
        signature_url: SIGNED_SIGNATURE_URL.into(),
    }
}

/// The update for `download_url`, verified by the manifest and signature at the two
/// other URLs.
fn update_info_for(
    download_url: String,
    manifest_url: String,
    signature_url: String,
) -> UpdateInfo {
    UpdateInfo {
        version: "2.0.0".into(),
        tag_name: "v2.0.0".into(),
        release_notes: "Fixes".into(),
        download_url,
        asset_size: ASSET_BYTES.len() as i64,
        asset_name: "EmuSync-x86_64.AppImage".into(),
        manifest_url,
        signature_url,
    }
}

/// A server with the test-signed manifest of v2.0.0 listing [`ASSET_BYTES`].
async fn signed_files_for_asset() -> SignedFilesServer {
    let (manifest, signature) =
        signed_manifest("v2.0.0", &[("EmuSync-x86_64.AppImage", ASSET_BYTES)]);
    SignedFilesServer::serve(manifest, signature).await
}

/// Advances the paused clock and resumes it, so later loopback I/O is not raced by the
/// automatic advance of a paused runtime.
async fn advance_virtual_time(duration: Duration) {
    tokio::time::pause();
    tokio::time::advance(duration).await;
    tokio::time::resume();
}

/// Gives the client a moment of real time to read bytes the server already wrote.
///
/// Nothing observable confirms the read; if it has not happened, the test that waits for
/// a stalled body exercises the wait for headers instead, which can never fail falsely.
async fn give_client_time_to_read_pending_bytes() {
    tokio::time::sleep(Duration::from_millis(200)).await;
}

#[tokio::test]
async fn https_only_check_against_plain_http_fails_without_connecting() {
    let server = TestServer::bind();
    let lowercase_base = server.base_url();
    let uppercase_base = lowercase_base.replacen("http://", "HTTP://", 1);

    for base_url in [lowercase_base, uppercase_base] {
        let source = GitHubUpdateSource::new(https_only_client(), "owner", "repo", "1.0.0", false)
            .with_base_url(base_url.clone());

        let result = tokio::time::timeout(common::HARNESS_WAIT_LIMIT, source.check())
            .await
            .unwrap_or_else(|_| {
                panic!("{base_url}: an HTTPS-only check of a plain HTTP URL must not wait on it")
            });

        match result {
            Err(UpdateError::Network(message)) => assert!(
                message.contains(REFUSED_SCHEME_CAUSE),
                "{base_url}: the error must give the refused scheme as its cause, got: {message}"
            ),
            other => {
                panic!("{base_url}: a refused check is an error, not \"up to date\", got {other:?}")
            }
        }
        server.assert_no_connection();
    }
}

#[tokio::test]
async fn https_only_download_from_plain_http_fails_without_connecting_or_creating_files() {
    let server = TestServer::bind();
    let source = GitHubUpdateSource::new(https_only_client(), "owner", "repo", "1.0.0", false);
    let info = update_info_for(
        server.url("/download/EmuSync-x86_64.AppImage"),
        server.url("/download/EmuSync-SHA256SUMS"),
        server.url("/download/EmuSync-SHA256SUMS.sig"),
    );

    // A first update with no previous file, and one replacing an earlier download.
    for previous in [None, Some(b"PREVIOUS".as_slice())] {
        let dir = tempdir().expect("create update tempdir");
        let destination = dir.path().join("update.AppImage");
        if let Some(bytes) = previous {
            std::fs::write(&destination, bytes).expect("write previous update");
        }
        let progress = Arc::new(Mutex::new(Vec::new()));
        let progress_sink = progress.clone();
        let destination_text = destination.to_string_lossy().into_owned();

        let result = tokio::time::timeout(
            common::HARNESS_WAIT_LIMIT,
            source.download(&info, &destination_text, move |value| {
                progress_sink.lock().expect("lock progress").push(value);
            }),
        )
        .await
        .expect("an HTTPS-only download of a plain HTTP URL must not wait on it");

        match result {
            Err(UpdateError::Network(message)) => assert!(
                message.contains(REFUSED_SCHEME_CAUSE),
                "the error must give the refused scheme as its cause, got: {message}"
            ),
            other => panic!("a plain HTTP download must be refused, got {other:?}"),
        }
        server.assert_no_connection();
        assert!(progress.lock().expect("lock progress").is_empty());
        assert!(part_entries(dir.path()).is_empty());
        match previous {
            None => assert_directory_entries(dir.path(), &[]),
            Some(bytes) => {
                assert_directory_entries(dir.path(), &["update.AppImage"]);
                assert_eq!(
                    std::fs::read(&destination).expect("read previous update"),
                    bytes
                );
            }
        }
    }
}

#[tokio::test]
async fn plain_http_client_checks_and_downloads_from_a_controlled_server() {
    let asset_server = TestServer::bind();
    let asset_url = asset_server.url("/download/EmuSync-x86_64.AppImage");
    let signed_files = signed_files_for_asset().await;
    let release_body = format!(
        r#"{{"tag_name":"v2.0.0","body":"Fixes","assets":[{{"name":"EmuSync-x86_64.AppImage","browser_download_url":"{asset_url}","size":{size}}},{{"name":"EmuSync-SHA256SUMS","browser_download_url":"{manifest_url}","size":1}},{{"name":"EmuSync-SHA256SUMS.sig","browser_download_url":"{signature_url}","size":1}}]}}"#,
        size = ASSET_BYTES.len(),
        manifest_url = signed_files.manifest_url,
        signature_url = signed_files.signature_url,
    );
    let release_server = TestServer::bind();
    let base_url = release_server.base_url();
    let release_task = release_server.spawn(move |mut connection| async move {
        connection
            .write_all(&response_with_content_length(
                "200 OK",
                release_body.as_bytes(),
            ))
            .await
    });
    let asset_task = asset_server.spawn(|mut connection| async move {
        connection
            .write_all(&response_with_content_length("200 OK", ASSET_BYTES))
            .await
    });
    let source = GitHubUpdateSource::new(plain_http_client(), "owner", "repo", "1.0.0", false)
        .with_base_url(base_url)
        .with_release_key(test_release_key());

    let update = source
        .check()
        .await
        .expect("check never fails")
        .expect("the controlled server publishes a newer release");
    assert_eq!(
        update,
        update_info_for(
            asset_url,
            signed_files.manifest_url.clone(),
            signed_files.signature_url.clone()
        )
    );

    let dir = tempdir().expect("create update tempdir");
    let destination = dir.path().join("update.AppImage");
    source
        .download(&update, &destination.to_string_lossy(), |_| {})
        .await
        .expect("download over plain HTTP succeeds");

    assert_eq!(
        std::fs::read(&destination).expect("read downloaded update"),
        ASSET_BYTES
    );
    assert_directory_entries(dir.path(), &["update.AppImage"]);
    let release_requests = release_task.finish().await;
    assert_eq!(release_requests.len(), 1);
    assert_eq!(release_requests[0].target, RELEASE_ENDPOINT);
    let asset_requests = asset_task.finish().await;
    assert_eq!(asset_requests.len(), 1);
    assert_eq!(
        asset_requests[0].target,
        "/download/EmuSync-x86_64.AppImage"
    );
}

#[tokio::test]
async fn plain_http_client_download_follows_a_plain_http_redirect() {
    let asset_server = TestServer::bind();
    let final_url = asset_server.url("/assets/final.AppImage");
    let asset_task = asset_server.spawn(|mut connection| async move {
        connection
            .write_all(&response_with_content_length("200 OK", ASSET_BYTES))
            .await
    });
    let redirecting_server = TestServer::bind();
    let signed_files = signed_files_for_asset().await;
    let info = update_info_for(
        redirecting_server.url("/download/EmuSync-x86_64.AppImage"),
        signed_files.manifest_url.clone(),
        signed_files.signature_url.clone(),
    );
    let redirecting_task = redirecting_server.spawn(move |mut connection| async move {
        connection
            .write_all(&http_response(
                "302 Found",
                &[
                    ("Location", final_url.as_str()),
                    ("Content-Length", "0"),
                    ("Connection", "close"),
                ],
                b"",
            ))
            .await
    });
    let dir = tempdir().expect("create update tempdir");
    let destination = dir.path().join("update.AppImage");
    std::fs::write(&destination, b"PREVIOUS").expect("write previous update");
    let source = GitHubUpdateSource::new(plain_http_client(), "owner", "repo", "1.0.0", false)
        .with_release_key(test_release_key());

    let result = tokio::time::timeout(
        common::HARNESS_WAIT_LIMIT,
        source.download(&info, &destination.to_string_lossy(), |_| {}),
    )
    .await
    .expect("redirected download finishes");

    result.expect("download through a plain HTTP redirect succeeds");
    assert_eq!(
        std::fs::read(&destination).expect("read downloaded update"),
        ASSET_BYTES
    );
    assert_directory_entries(dir.path(), &["update.AppImage"]);
    let redirecting_requests = redirecting_task.finish().await;
    assert_eq!(redirecting_requests.len(), 1);
    assert_eq!(
        redirecting_requests[0].target,
        "/download/EmuSync-x86_64.AppImage"
    );
    let asset_requests = asset_task.finish().await;
    assert_eq!(asset_requests.len(), 1);
    assert_eq!(asset_requests[0].target, "/assets/final.AppImage");
}

#[tokio::test]
async fn check_fails_within_check_limit_when_server_accepts_and_never_answers() {
    let server = TestServer::bind();
    let base_url = server.base_url();
    let (release_tx, release_rx) = oneshot::channel::<()>();
    let mut server_task = server.spawn(move |connection| async move {
        // Accept the request and hold the socket open without answering.
        let _connection = connection;
        let _ = release_rx.await;
        Ok(())
    });
    let source = GitHubUpdateSource::new(plain_http_client(), "owner", "repo", "1.0.0", false)
        .with_base_url(base_url);
    let mut check_task = tokio::spawn(async move { source.check().await });

    let request = server_task
        .wait_for_request(&mut check_task, "silent release server")
        .await;
    assert_eq!(request.target, RELEASE_ENDPOINT);
    assert!(
        !check_task.is_finished(),
        "the check must still be waiting for the silent server"
    );

    advance_virtual_time(SPECIFIED_CHECK_LIMIT + Duration::from_secs(1)).await;

    let result = join_with_limit(&mut check_task, &mut server_task, "silent release server").await;
    match result {
        Err(UpdateError::Network(message)) => assert!(
            message.contains(CHECK_TIMEOUT_CAUSE),
            "the error must give the timeout as its cause, got: {message}"
        ),
        other => panic!("a check that outlived its limit is an error, got {other:?}"),
    }
    drop(release_tx);
    assert_eq!(server_task.finish().await.len(), 1);
}

#[tokio::test]
async fn check_fails_within_check_limit_when_body_stalls_after_headers() {
    let half = RELEASE_BODY.len() / 2;
    let declared_length = RELEASE_BODY.len().to_string();
    let server = TestServer::bind();
    let base_url = server.base_url();
    let (headers_sent_tx, headers_sent_rx) = oneshot::channel::<()>();
    let (release_tx, release_rx) = oneshot::channel::<()>();
    let mut server_task = server.spawn(move |mut connection| async move {
        connection
            .write_all(&http_response(
                "200 OK",
                &[
                    ("Content-Type", "application/json"),
                    ("Content-Length", declared_length.as_str()),
                ],
                &RELEASE_BODY.as_bytes()[..half],
            ))
            .await?;
        let _ = headers_sent_tx.send(());
        let _ = release_rx.await;
        Ok(())
    });
    let source = GitHubUpdateSource::new(plain_http_client(), "owner", "repo", "1.0.0", false)
        .with_base_url(base_url);
    let mut check_task = tokio::spawn(async move { source.check().await });

    server_task
        .wait_for_request(&mut check_task, "stalled release body")
        .await;
    tokio::time::timeout(common::HARNESS_WAIT_LIMIT, headers_sent_rx)
        .await
        .expect("server sends the headers within the harness limit")
        .expect("server reports the headers as sent");
    give_client_time_to_read_pending_bytes().await;
    assert!(
        !check_task.is_finished(),
        "the check must still be waiting for the rest of the body"
    );

    advance_virtual_time(SPECIFIED_CHECK_LIMIT + Duration::from_secs(1)).await;

    let result = join_with_limit(&mut check_task, &mut server_task, "stalled release body").await;
    match result {
        Err(UpdateError::Network(message)) => assert!(
            message.contains(CHECK_TIMEOUT_CAUSE),
            "the error must give the timeout as its cause, got: {message}"
        ),
        other => panic!("a check whose body stalled past its limit is an error, got {other:?}"),
    }
    drop(release_tx);
    assert_eq!(server_task.finish().await.len(), 1);
}

#[tokio::test]
async fn check_accepts_a_slow_release_response_that_completes_before_the_limit() {
    let half = RELEASE_BODY.len() / 2;
    let declared_length = RELEASE_BODY.len().to_string();
    let server = TestServer::bind();
    let base_url = server.base_url();
    let (rest_tx, rest_rx) = oneshot::channel::<()>();
    let mut server_task = server.spawn(move |mut connection| async move {
        connection
            .write_all(&http_response(
                "200 OK",
                &[
                    ("Content-Type", "application/json"),
                    ("Content-Length", declared_length.as_str()),
                    ("Connection", "close"),
                ],
                &RELEASE_BODY.as_bytes()[..half],
            ))
            .await?;
        if rest_rx.await.is_ok() {
            connection
                .write_all(&RELEASE_BODY.as_bytes()[half..])
                .await?;
        }
        Ok(())
    });
    let source = GitHubUpdateSource::new(plain_http_client(), "owner", "repo", "1.0.0", false)
        .with_base_url(base_url);
    let mut check_task = tokio::spawn(async move { source.check().await });

    server_task
        .wait_for_request(&mut check_task, "slow release body")
        .await;
    // Ten seconds of margin for the real time the rest of the exchange takes.
    advance_virtual_time(SPECIFIED_CHECK_LIMIT - Duration::from_secs(10)).await;
    rest_tx
        .send(())
        .expect("server waits for the rest of the body");

    let result = join_with_limit(&mut check_task, &mut server_task, "slow release body").await;
    assert_eq!(
        result.expect("check never fails"),
        Some(expected_release_update()),
        "a response finished before the limit must still be read"
    );
    assert_eq!(server_task.finish().await.len(), 1);
}

#[tokio::test]
async fn release_client_download_that_keeps_receiving_data_has_no_overall_limit() {
    // Each gap stays below the downloader's inactivity limit, while the whole transfer
    // lasts far longer than any overall limit a client could reasonably impose.
    const CHUNK_COUNT: usize = 20;
    const GAP_BETWEEN_CHUNKS: Duration = Duration::from_secs(55);
    const CHUNK_BYTES: &[u8] = b"0123456789";
    let total_length = (CHUNK_COUNT * CHUNK_BYTES.len()).to_string();

    let server = TestServer::bind();
    let url = server.url("/asset");
    let (next_chunk_tx, mut next_chunk_rx) = mpsc::unbounded_channel::<()>();
    let mut server_task = server.spawn(move |mut connection| async move {
        connection
            .write_all(&http_response(
                "200 OK",
                &[
                    ("Content-Length", total_length.as_str()),
                    ("Connection", "close"),
                ],
                CHUNK_BYTES,
            ))
            .await?;
        for _ in 1..CHUNK_COUNT {
            if next_chunk_rx.recv().await.is_none() {
                return Ok(());
            }
            connection.write_all(CHUNK_BYTES).await?;
        }
        Ok(())
    });
    let dir = tempdir().expect("create update tempdir");
    let destination = dir.path().join("update.AppImage");
    let destination_for_task = destination.clone();
    let downloader = UpdateDownloader::new(plain_http_client());
    let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<f32>();
    let mut download_task = tokio::spawn(async move {
        downloader
            .download(&url, &destination_for_task, move |value| {
                let _ = progress_tx.send(value);
            })
            .await
    });

    tokio::time::pause();
    let transfer_start = tokio::time::Instant::now();
    tokio::time::resume();
    for chunk_number in 1..=CHUNK_COUNT {
        let expected_progress = chunk_number as f32 / CHUNK_COUNT as f32;
        loop {
            let progress = recv_with_limit(
                &mut progress_rx,
                &mut download_task,
                &mut server_task,
                "progressing download",
            )
            .await;
            if progress >= expected_progress - 1e-6 {
                break;
            }
        }
        if chunk_number < CHUNK_COUNT {
            advance_virtual_time(GAP_BETWEEN_CHUNKS).await;
            next_chunk_tx
                .send(())
                .expect("server waits for the next chunk");
        }
    }

    let result =
        join_with_limit(&mut download_task, &mut server_task, "progressing download").await;
    result.expect("a download that keeps receiving data must complete");
    tokio::time::pause();
    let transfer_duration = transfer_start.elapsed();
    tokio::time::resume();
    assert!(
        transfer_duration >= Duration::from_secs(1045),
        "the transfer must span 19 gaps of 55 s of virtual time, lasted {transfer_duration:?}"
    );
    assert_eq!(
        std::fs::read(&destination).expect("read downloaded update"),
        CHUNK_BYTES.repeat(CHUNK_COUNT)
    );
    assert_directory_entries(dir.path(), &["update.AppImage"]);
    assert_eq!(server_task.finish().await.len(), 1);
}

#[tokio::test]
async fn release_client_download_stalled_after_headers_fails_with_downloader_stall_error() {
    let server = TestServer::bind();
    let url = server.url("/asset");
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
    let destination_for_task = destination.clone();
    let downloader = UpdateDownloader::new(plain_http_client());
    let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<f32>();
    let mut download_task = tokio::spawn(async move {
        downloader
            .download(&url, &destination_for_task, move |value| {
                let _ = progress_tx.send(value);
            })
            .await
    });

    loop {
        let progress = recv_with_limit(
            &mut progress_rx,
            &mut download_task,
            &mut server_task,
            "first half of the body",
        )
        .await;
        assert!(
            progress <= 0.5,
            "only half of the body was sent: {progress}"
        );
        if progress == 0.5 {
            break;
        }
    }

    advance_virtual_time(SPECIFIED_INACTIVITY_LIMIT + Duration::from_secs(1)).await;

    let result = join_with_limit(&mut download_task, &mut server_task, "stalled body").await;
    match result {
        Err(UpdateError::Network(message)) => assert!(
            message.starts_with("download stalled: no data received for"),
            "the stall must be detected by the downloader, got: {message}"
        ),
        other => panic!("expected the downloader's stall error, got {other:?}"),
    }
    drop(release_tx);
    server_task.finish().await;
    assert_eq!(
        std::fs::read(&destination).expect("read previous update"),
        b"PREVIOUS"
    );
    assert_directory_entries(dir.path(), &["update.AppImage"]);
}

#[tokio::test]
async fn release_client_download_without_headers_fails_with_downloader_inactivity_error() {
    let server = TestServer::bind();
    let url = server.url("/asset");
    let (release_tx, release_rx) = oneshot::channel::<()>();
    let mut server_task = server.spawn(move |connection| async move {
        let _connection = connection;
        let _ = release_rx.await;
        Ok(())
    });
    let dir = tempdir().expect("create update tempdir");
    let destination = dir.path().join("update.AppImage");
    let destination_for_task = destination.clone();
    let downloader = UpdateDownloader::new(plain_http_client());
    let mut download_task = tokio::spawn(async move {
        downloader
            .download(&url, &destination_for_task, |_| {})
            .await
    });

    server_task
        .wait_for_request(&mut download_task, "silent asset server")
        .await;
    advance_virtual_time(SPECIFIED_INACTIVITY_LIMIT + Duration::from_secs(1)).await;

    let result = join_with_limit(&mut download_task, &mut server_task, "silent asset server").await;
    match result {
        Err(UpdateError::Network(message)) => assert!(
            message.starts_with("download request timed out after")
                && message.ends_with("of inactivity"),
            "the silent server must be detected by the downloader, got: {message}"
        ),
        other => panic!("expected the downloader's inactivity error, got {other:?}"),
    }
    drop(release_tx);
    server_task.finish().await;
    assert_directory_entries(dir.path(), &[]);
}
