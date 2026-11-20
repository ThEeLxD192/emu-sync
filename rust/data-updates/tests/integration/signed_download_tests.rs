//! `GitHubUpdateSource` against hostile release servers: the asset is fetched only after
//! a signed manifest of its release lists it, the manifest and signature are bounded, and
//! bytes other than the signed ones never reach the destination.

use crate::common::{
    HARNESS_WAIT_LIMIT, client, directory_entries, http_response, response_with_content_length,
    signed_release::{sign_with_test_key, test_release_key},
};
use emusync_data_updates::{GitHubUpdateSource, UpdateError};
use emusync_domain::update::{UpdateInfo, UpdateSource};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
use tempfile::tempdir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::{JoinHandle, JoinSet},
};

const APPIMAGE: &str = "EmuSync-x86_64.AppImage";
const RELEASE_PATH: &str = "/repos/owner/repo/releases/latest";
const MANIFEST_PATH: &str = "/manifest";
const SIGNATURE_PATH: &str = "/signature";
const ASSET_PATH: &str = "/asset";

/// The asset the signed manifests list, and its SHA-256 (FIPS 180-2 test vector).
const ASSET_BYTES: &[u8] = b"abc";
const ASSET_SHA256_HEX: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
/// SHA-256 of the empty string, for filler lines.
const EMPTY_SHA256_HEX: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// The largest manifest or signature the contract accepts.
const SIGNED_FILE_LIMIT_BYTES: usize = 64 * 1024;

/// A loopback server answering each path with a fixed raw HTTP response (404 for any
/// other path) and recording every path requested, until dropped.
struct RoutedServer {
    base_url: String,
    requested_paths: Arc<Mutex<Vec<String>>>,
    task: JoinHandle<()>,
}

impl RoutedServer {
    async fn serve(routes: Vec<(&'static str, Vec<u8>)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind routed listener");
        let base_url = format!("http://{}", listener.local_addr().expect("routed address"));
        let requested_paths = Arc::new(Mutex::new(Vec::new()));
        let recorded_paths = requested_paths.clone();
        let routes = Arc::new(routes);
        let task = tokio::spawn(async move {
            // Dropping the set when this task is aborted aborts every open connection.
            let mut connections = JoinSet::new();
            while let Ok((stream, _)) = listener.accept().await {
                connections.spawn(answer(stream, routes.clone(), recorded_paths.clone()));
            }
        });
        Self {
            base_url,
            requested_paths,
            task,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    /// Every path requested so far. A request the client got an answer to is already
    /// here, since the path is recorded before the answer is written.
    fn requested_paths(&self) -> Vec<String> {
        self.requested_paths
            .lock()
            .expect("lock requested paths")
            .clone()
    }
}

impl Drop for RoutedServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn answer(
    mut stream: TcpStream,
    routes: Arc<Vec<(&'static str, Vec<u8>)>>,
    recorded_paths: Arc<Mutex<Vec<String>>>,
) {
    let mut head = Vec::new();
    let mut chunk = [0_u8; 1024];
    while !head.windows(4).any(|window| window == b"\r\n\r\n") {
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(read) => head.extend_from_slice(&chunk[..read]),
        }
    }
    let head = String::from_utf8_lossy(&head);
    let path = head
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_owned();
    let response = routes
        .iter()
        .find(|(route, _)| *route == path)
        .map(|(_, response)| response.clone())
        .unwrap_or_else(|| answer_with_status("404 Not Found"));
    recorded_paths
        .lock()
        .expect("lock requested paths")
        .push(path);
    // The client may hang up early on a body it refuses.
    let _ = stream.write_all(&response).await;
    let _ = stream.shutdown().await;
}

fn answer_with_length(body: &[u8]) -> Vec<u8> {
    response_with_content_length("200 OK", body)
}

/// A body whose end is only the connection closing: no `Content-Length` to refuse early.
fn answer_until_close(body: &[u8]) -> Vec<u8> {
    http_response("200 OK", &[("Connection", "close")], body)
}

fn answer_with_status(status_line: &str) -> Vec<u8> {
    response_with_content_length(status_line, b"")
}

/// The manifest of `tag` listing [`ASSET_BYTES`] as the AppImage, written by hand.
fn manifest_of(tag: &str) -> Vec<u8> {
    format!("EmuSync release {tag}\n{ASSET_SHA256_HEX}  {APPIMAGE}\n").into_bytes()
}

/// A valid manifest of v2.0.0 for [`ASSET_BYTES`], padded with the line of another asset
/// to exactly `size_bytes`.
fn manifest_of_size(size_bytes: usize) -> Vec<u8> {
    let mut manifest = manifest_of("v2.0.0");
    let filler_name_length = size_bytes - manifest.len() - EMPTY_SHA256_HEX.len() - "  \n".len();
    manifest.extend_from_slice(
        format!("{EMPTY_SHA256_HEX}  {}\n", "f".repeat(filler_name_length)).as_bytes(),
    );
    assert_eq!(manifest.len(), size_bytes, "filler arithmetic");
    manifest
}

/// The test signature of `manifest`, padded with spaces to exactly `size_bytes`.
fn signature_of_size(manifest: &[u8], size_bytes: usize) -> Vec<u8> {
    let mut signature = sign_with_test_key(manifest);
    signature.resize(size_bytes, b' ');
    signature
}

fn update_info(server: &RoutedServer) -> UpdateInfo {
    UpdateInfo {
        version: "2.0.0".into(),
        tag_name: "v2.0.0".into(),
        release_notes: "notes".into(),
        download_url: server.url(ASSET_PATH),
        asset_size: ASSET_BYTES.len() as i64,
        asset_name: APPIMAGE.into(),
        manifest_url: server.url(MANIFEST_PATH),
        signature_url: server.url(SIGNATURE_PATH),
    }
}

fn test_key_source() -> GitHubUpdateSource {
    GitHubUpdateSource::new(client(), "owner", "repo", "1.0.0", false)
        .with_release_key(test_release_key())
}

async fn download_into(
    source: &GitHubUpdateSource,
    info: &UpdateInfo,
    destination: &Path,
) -> Result<(), UpdateError> {
    let destination = destination.to_string_lossy().into_owned();
    tokio::time::timeout(
        HARNESS_WAIT_LIMIT,
        source.download(info, &destination, |_| {}),
    )
    .await
    .expect("the download ends within the harness limit")
}

/// Serves the release files, downloads over a previous update and returns the result
/// and the paths requested.
async fn download_over_previous(
    source: &GitHubUpdateSource,
    routes: Vec<(&'static str, Vec<u8>)>,
    case: &str,
) -> (Result<(), UpdateError>, Vec<String>, tempfile::TempDir) {
    let server = RoutedServer::serve(routes).await;
    let dir = tempdir().expect("create update tempdir");
    let destination = dir.path().join("update.AppImage");
    std::fs::write(&destination, b"PREVIOUS").unwrap_or_else(|error| {
        panic!("{case}: write previous update: {error}");
    });
    let result = download_into(source, &update_info(&server), &destination).await;
    (result, server.requested_paths(), dir)
}

fn assert_previous_update_kept(dir: &Path, case: &str) {
    assert_eq!(
        std::fs::read(dir.join("update.AppImage")).expect("read destination"),
        b"PREVIOUS",
        "{case}: the previous update must stay as it was"
    );
    assert_eq!(
        directory_entries(dir),
        vec!["update.AppImage".to_owned()],
        "{case}: no staging file may be left behind"
    );
}

fn assert_asset_not_requested(requested_paths: &[String], case: &str) {
    assert!(
        !requested_paths.iter().any(|path| path == ASSET_PATH),
        "{case}: the asset must not be fetched before its manifest verifies, requested {requested_paths:?}"
    );
}

#[tokio::test]
async fn an_asset_matching_its_signed_hash_replaces_the_previous_update() {
    let manifest = manifest_of("v2.0.0");
    let signature = sign_with_test_key(&manifest);

    let (result, requested_paths, dir) = download_over_previous(
        &test_key_source(),
        vec![
            (MANIFEST_PATH, answer_with_length(&manifest)),
            (SIGNATURE_PATH, answer_with_length(&signature)),
            (ASSET_PATH, answer_with_length(ASSET_BYTES)),
        ],
        "matching asset",
    )
    .await;

    result.expect("an asset with the signed hash is installed");
    assert_eq!(
        std::fs::read(dir.path().join("update.AppImage")).expect("read destination"),
        ASSET_BYTES
    );
    assert_eq!(directory_entries(dir.path()), vec!["update.AppImage"]);
    assert_eq!(requested_paths.len(), 3, "requested {requested_paths:?}");
    assert_eq!(
        requested_paths.last().map(String::as_str),
        Some(ASSET_PATH),
        "the asset comes after its manifest and signature"
    );
}

#[tokio::test]
async fn an_asset_whose_bytes_differ_from_the_signed_hash_never_reaches_the_destination() {
    let manifest = manifest_of("v2.0.0");
    let signature = sign_with_test_key(&manifest);

    for (case, asset_response) in [
        ("same length, one byte changed", answer_with_length(b"abd")),
        ("one byte appended", answer_with_length(b"abcd")),
        ("one byte missing", answer_with_length(b"ab")),
        ("no Content-Length", answer_until_close(b"abd")),
    ] {
        let (result, requested_paths, dir) = download_over_previous(
            &test_key_source(),
            vec![
                (MANIFEST_PATH, answer_with_length(&manifest)),
                (SIGNATURE_PATH, answer_with_length(&signature)),
                (ASSET_PATH, asset_response),
            ],
            case,
        )
        .await;

        match result {
            Err(UpdateError::Verification(_)) => {}
            other => panic!("{case}: expected a verification error, got {other:?}"),
        }
        assert!(
            requested_paths.iter().any(|path| path == ASSET_PATH),
            "{case}: the asset was fetched and refused"
        );
        assert_previous_update_kept(dir.path(), case);
    }
}

#[tokio::test]
async fn an_asset_that_does_not_match_leaves_no_file_when_there_was_no_previous_update() {
    let manifest = manifest_of("v2.0.0");
    let server = RoutedServer::serve(vec![
        (MANIFEST_PATH, answer_with_length(&manifest)),
        (
            SIGNATURE_PATH,
            answer_with_length(&sign_with_test_key(&manifest)),
        ),
        (ASSET_PATH, answer_with_length(b"xyz")),
    ])
    .await;
    let dir = tempdir().expect("create update tempdir");

    let result = download_into(
        &test_key_source(),
        &update_info(&server),
        &dir.path().join("update.AppImage"),
    )
    .await;

    assert!(
        matches!(result, Err(UpdateError::Verification(_))),
        "expected a verification error, got {result:?}"
    );
    assert_eq!(directory_entries(dir.path()), Vec::<String>::new());
}

#[tokio::test]
async fn a_manifest_or_signature_the_server_does_not_return_stops_before_the_asset() {
    let manifest = manifest_of("v2.0.0");
    let signature = sign_with_test_key(&manifest);

    for (case, manifest_response, signature_response, expected_status) in [
        (
            "manifest 404",
            answer_with_status("404 Not Found"),
            answer_with_length(&signature),
            404,
        ),
        (
            "manifest 500",
            answer_with_status("500 Internal Server Error"),
            answer_with_length(&signature),
            500,
        ),
        (
            "signature 404",
            answer_with_length(&manifest),
            answer_with_status("404 Not Found"),
            404,
        ),
        (
            "signature 500",
            answer_with_length(&manifest),
            answer_with_status("500 Internal Server Error"),
            500,
        ),
    ] {
        let (result, requested_paths, dir) = download_over_previous(
            &test_key_source(),
            vec![
                (MANIFEST_PATH, manifest_response),
                (SIGNATURE_PATH, signature_response),
                (ASSET_PATH, answer_with_length(ASSET_BYTES)),
            ],
            case,
        )
        .await;

        match result {
            Err(UpdateError::Http(status)) if status.as_u16() == expected_status => {}
            other => panic!("{case}: expected HTTP {expected_status}, got {other:?}"),
        }
        assert_asset_not_requested(&requested_paths, case);
        assert_previous_update_kept(dir.path(), case);
    }
}

#[tokio::test]
async fn a_manifest_or_signature_over_64_kib_is_refused_before_the_asset() {
    let oversized_manifest = manifest_of_size(SIGNED_FILE_LIMIT_BYTES + 1);
    let oversized_manifest_signature = sign_with_test_key(&oversized_manifest);
    let huge_manifest = manifest_of_size(1024 * 1024);
    let huge_manifest_signature = sign_with_test_key(&huge_manifest);
    let manifest = manifest_of("v2.0.0");
    let padded_signature = signature_of_size(&manifest, SIGNED_FILE_LIMIT_BYTES + 1);

    // Each manifest and signature would verify if it were read whole.
    for (case, manifest_response, signature_response) in [
        (
            "manifest of 64 KiB + 1 with Content-Length",
            answer_with_length(&oversized_manifest),
            answer_with_length(&oversized_manifest_signature),
        ),
        (
            "manifest of 64 KiB + 1 until close",
            answer_until_close(&oversized_manifest),
            answer_with_length(&oversized_manifest_signature),
        ),
        (
            "manifest of 1 MiB until close",
            answer_until_close(&huge_manifest),
            answer_with_length(&huge_manifest_signature),
        ),
        (
            "signature padded to 64 KiB + 1 with Content-Length",
            answer_with_length(&manifest),
            answer_with_length(&padded_signature),
        ),
        (
            "signature padded to 64 KiB + 1 until close",
            answer_with_length(&manifest),
            answer_until_close(&padded_signature),
        ),
    ] {
        let (result, requested_paths, dir) = download_over_previous(
            &test_key_source(),
            vec![
                (MANIFEST_PATH, manifest_response),
                (SIGNATURE_PATH, signature_response),
                (ASSET_PATH, answer_with_length(ASSET_BYTES)),
            ],
            case,
        )
        .await;

        match result {
            Err(UpdateError::Verification(_)) => {}
            other => panic!("{case}: expected a verification error, got {other:?}"),
        }
        assert_asset_not_requested(&requested_paths, case);
        assert_previous_update_kept(dir.path(), case);
    }
}

#[tokio::test]
async fn a_manifest_and_signature_of_exactly_64_kib_are_accepted() {
    let full_manifest = manifest_of_size(SIGNED_FILE_LIMIT_BYTES);
    let full_manifest_signature = sign_with_test_key(&full_manifest);
    let manifest = manifest_of("v2.0.0");
    let full_signature = signature_of_size(&manifest, SIGNED_FILE_LIMIT_BYTES);

    for (case, manifest_response, signature_response) in [
        (
            "manifest of 64 KiB with Content-Length",
            answer_with_length(&full_manifest),
            answer_with_length(&full_manifest_signature),
        ),
        (
            "manifest of 64 KiB until close",
            answer_until_close(&full_manifest),
            answer_with_length(&full_manifest_signature),
        ),
        (
            "signature padded to 64 KiB until close",
            answer_with_length(&manifest),
            answer_until_close(&full_signature),
        ),
    ] {
        let (result, _, dir) = download_over_previous(
            &test_key_source(),
            vec![
                (MANIFEST_PATH, manifest_response),
                (SIGNATURE_PATH, signature_response),
                (ASSET_PATH, answer_with_length(ASSET_BYTES)),
            ],
            case,
        )
        .await;

        result.unwrap_or_else(|error| panic!("{case}: expected the update to install: {error}"));
        assert_eq!(
            std::fs::read(dir.path().join("update.AppImage")).expect("read destination"),
            ASSET_BYTES,
            "{case}"
        );
    }
}

#[tokio::test]
async fn the_signed_manifest_of_an_earlier_release_does_not_install_the_newer_one() {
    // An attacker replays the genuine, signed manifest of v1.9.0 with v1.9.0's asset
    // served as the v2.0.0 download.
    let earlier_manifest = manifest_of("v1.9.0");

    let (result, requested_paths, dir) = download_over_previous(
        &test_key_source(),
        vec![
            (MANIFEST_PATH, answer_with_length(&earlier_manifest)),
            (
                SIGNATURE_PATH,
                answer_with_length(&sign_with_test_key(&earlier_manifest)),
            ),
            (ASSET_PATH, answer_with_length(ASSET_BYTES)),
        ],
        "replayed manifest",
    )
    .await;

    assert!(
        matches!(result, Err(UpdateError::Verification(_))),
        "expected a verification error, got {result:?}"
    );
    assert_asset_not_requested(&requested_paths, "replayed manifest");
    assert_previous_update_kept(dir.path(), "replayed manifest");
}

#[tokio::test]
async fn the_default_source_refuses_a_manifest_signed_by_the_published_test_key() {
    let manifest = manifest_of("v2.0.0");
    let default_source = GitHubUpdateSource::new(client(), "owner", "repo", "1.0.0", false);

    let (result, requested_paths, dir) = download_over_previous(
        &default_source,
        vec![
            (MANIFEST_PATH, answer_with_length(&manifest)),
            (
                SIGNATURE_PATH,
                answer_with_length(&sign_with_test_key(&manifest)),
            ),
            (ASSET_PATH, answer_with_length(ASSET_BYTES)),
        ],
        "test key against the EmuSync key",
    )
    .await;

    assert!(
        matches!(result, Err(UpdateError::Verification(_))),
        "expected a verification error, got {result:?}"
    );
    assert_asset_not_requested(&requested_paths, "test key against the EmuSync key");
    assert_previous_update_kept(dir.path(), "test key against the EmuSync key");
}

#[tokio::test]
async fn check_refuses_a_newer_release_without_both_signed_files() {
    let appimage = r#"{"name":"EmuSync-x86_64.AppImage","browser_download_url":"https://example.com/EmuSync-x86_64.AppImage","size":3}"#;
    let manifest = r#"{"name":"EmuSync-SHA256SUMS","browser_download_url":"https://example.com/EmuSync-SHA256SUMS","size":200}"#;
    let signature = r#"{"name":"EmuSync-SHA256SUMS.sig","browser_download_url":"https://example.com/EmuSync-SHA256SUMS.sig","size":89}"#;
    let lookalike_manifest = r#"{"name":"EmuSync-SHA256SUMS.txt","browser_download_url":"https://example.com/EmuSync-SHA256SUMS.txt","size":200}"#;
    let lookalike_signature = r#"{"name":"emusync-sha256sums.sig","browser_download_url":"https://example.com/emusync-sha256sums.sig","size":89}"#;

    for (case, assets) in [
        ("manifest only", format!("{appimage},{manifest}")),
        ("signature only", format!("{appimage},{signature}")),
        (
            "look-alike manifest name",
            format!("{appimage},{lookalike_manifest},{signature}"),
        ),
        (
            "look-alike signature name",
            format!("{appimage},{manifest},{lookalike_signature}"),
        ),
    ] {
        let release = format!(r#"{{"tag_name":"v2.0.0","assets":[{assets}]}}"#);
        let server =
            RoutedServer::serve(vec![(RELEASE_PATH, answer_with_length(release.as_bytes()))]).await;
        let source = GitHubUpdateSource::new(client(), "owner", "repo", "1.0.0", false)
            .with_base_url(server.base_url.clone());

        let result = tokio::time::timeout(HARNESS_WAIT_LIMIT, source.check())
            .await
            .expect("the check ends within the harness limit");

        match result {
            Err(UpdateError::Verification(_)) => {}
            other => {
                panic!("{case}: a release that cannot be verified is not an update, got {other:?}")
            }
        }
        assert_eq!(server.requested_paths(), vec![RELEASE_PATH], "{case}");
    }
}
