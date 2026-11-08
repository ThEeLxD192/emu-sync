mod common;

use common::{
    TestServer, assert_directory_entries, client, http_response, response_with_content_length,
};
use emusync_data_updates::{GitHubUpdateSource, UpdateError};
use emusync_domain::update::{UpdateInfo, UpdateSource};
use tempfile::tempdir;

const RELEASE_ENDPOINT: &str = "/repos/owner/repo/releases/latest";

fn release_fixture() -> &'static str {
    r#"{
        "tag_name": "v0.3.0",
        "name": "EmuSync 0.3.0",
        "body": "Bug fixes and improvements",
        "assets": [
            {
                "name": "EmuSync-x86_64.AppImage",
                "browser_download_url": "https://example.com/download/EmuSync-x86_64.AppImage",
                "size": 1024
            },
            {
                "name": "EmuSync-0.3.0.msi",
                "browser_download_url": "https://example.com/download/EmuSync-0.3.0.msi",
                "size": 2048
            }
        ]
    }"#
}

fn assert_release_request(requests: &[common::RequestRecord]) {
    assert_eq!(requests.len(), 1, "expected exactly one release request");
    let request = &requests[0];
    assert_eq!(request.method, "GET");
    assert_eq!(request.target, RELEASE_ENDPOINT);
    assert_eq!(
        request.header("Accept"),
        Some("application/vnd.github+json")
    );
    assert_eq!(request.header("User-Agent"), Some("EmuSync-Desktop-App"));
}

async fn check_release(
    body: &'static str,
    is_windows: bool,
) -> (
    Result<Option<UpdateInfo>, UpdateError>,
    Vec<common::RequestRecord>,
) {
    let server = TestServer::bind();
    let base_url = server.base_url();
    let server_task = server.spawn(move |mut connection| async move {
        connection
            .write_all(&response_with_content_length("200 OK", body.as_bytes()))
            .await?;
        Ok(())
    });
    let source = GitHubUpdateSource::new(client(), "owner", "repo", "0.2.0", is_windows)
        .with_base_url(base_url);
    let result = source.check().await;
    let requests = server_task.finish().await;
    (result, requests)
}

#[tokio::test]
async fn check_for_updates_detects_newer_release_and_appimage_on_linux() {
    let (result, requests) = check_release(release_fixture(), false).await;
    let update = result.unwrap().expect("linux update available");
    assert_eq!(
        update,
        UpdateInfo {
            version: "0.3.0".into(),
            tag_name: "v0.3.0".into(),
            release_notes: "Bug fixes and improvements".into(),
            download_url: "https://example.com/download/EmuSync-x86_64.AppImage".into(),
            asset_size: 1024,
            asset_name: "EmuSync-x86_64.AppImage".into(),
        }
    );
    assert_release_request(&requests);
}

#[tokio::test]
async fn check_for_updates_detects_newer_release_and_msi_on_windows() {
    let (result, requests) = check_release(release_fixture(), true).await;
    let update = result.unwrap().expect("windows update available");
    assert_eq!(
        update,
        UpdateInfo {
            version: "0.3.0".into(),
            tag_name: "v0.3.0".into(),
            release_notes: "Bug fixes and improvements".into(),
            download_url: "https://example.com/download/EmuSync-0.3.0.msi".into(),
            asset_size: 2048,
            asset_name: "EmuSync-0.3.0.msi".into(),
        }
    );
    assert_release_request(&requests);
}

async fn assert_check_returns_none(name: &str, response: Option<Vec<u8>>) {
    let server = TestServer::bind();
    let base_url = server.base_url();
    let server_task = server.spawn(move |mut connection| async move {
        if let Some(response) = response {
            connection.write_all(&response).await?;
        }
        Ok(())
    });
    let source =
        GitHubUpdateSource::new(client(), "owner", "repo", "1.0.0", false).with_base_url(base_url);

    match source.check().await {
        Ok(None) => {}
        other => panic!("{name}: expected Kotlin-compatible Ok(None), got {other:?}"),
    }

    let requests = server_task.finish().await;
    assert_eq!(
        requests.len(),
        1,
        "{name}: expected only the release request, got {requests:?}"
    );
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].target, RELEASE_ENDPOINT);
}

#[tokio::test]
async fn check_degrades_transport_http_and_json_failures_to_none() {
    assert_check_returns_none("close without headers", None).await;

    for status_line in [
        "404 Not Found",
        "429 Too Many Requests",
        "503 Service Unavailable",
    ] {
        assert_check_returns_none(
            status_line,
            Some(http_response(
                status_line,
                &[("Content-Length", "0"), ("Connection", "close")],
                b"",
            )),
        )
        .await;
    }

    assert_check_returns_none(
        "invalid json",
        Some(response_with_content_length("200 OK", b"not json")),
    )
    .await;

    assert_check_returns_none(
        "missing tag_name",
        Some(response_with_content_length(
            "200 OK",
            br#"{"name":"EmuSync","assets":[]}"#,
        )),
    )
    .await;

    let truncated = br#"{"tag_name":"v2.0.0""#;
    let declared_length = (truncated.len() + 20).to_string();
    assert_check_returns_none(
        "truncated json body",
        Some(http_response(
            "200 OK",
            &[
                ("Content-Type", "application/json"),
                ("Content-Length", declared_length.as_str()),
                ("Connection", "close"),
            ],
            truncated,
        )),
    )
    .await;
}

async fn assert_check_none_for_release(body: String, is_windows: bool, name: &str) {
    let server = TestServer::bind();
    let base_url = server.base_url();
    let server_task = server.spawn(move |mut connection| async move {
        connection
            .write_all(&response_with_content_length("200 OK", body.as_bytes()))
            .await?;
        Ok(())
    });
    let source = GitHubUpdateSource::new(client(), "owner", "repo", "1.0.0", is_windows)
        .with_base_url(base_url);
    match source.check().await {
        Ok(None) => {}
        other => panic!("{name}: expected Ok(None), got {other:?}"),
    }
    let requests = server_task.finish().await;
    assert_eq!(requests.len(), 1, "{name}: expected one request");
    assert_eq!(requests[0].target, RELEASE_ENDPOINT);
}

#[tokio::test]
async fn check_returns_none_for_non_newer_or_unusable_releases() {
    let appimage = r#"{"name":"EmuSync-x86_64.AppImage","browser_download_url":"https://example.com/EmuSync-x86_64.AppImage","size":1024}"#;
    let msi = r#"{"name":"EmuSync.msi","browser_download_url":"https://example.com/EmuSync.msi","size":1024}"#;
    let text = r#"{"name":"release.txt","browser_download_url":"https://example.com/release.txt","size":12}"#;

    let cases = vec![
        (
            format!(r#"{{"tag_name":"v1.0.0","assets":[{appimage}]}}"#),
            false,
            "same version",
        ),
        (
            format!(r#"{{"tag_name":"v0.9.0","assets":[{appimage}]}}"#),
            false,
            "older version",
        ),
        (
            r#"{"tag_name":"v2.0.0","assets":[]}"#.to_owned(),
            false,
            "newer version without assets",
        ),
        (
            format!(r#"{{"tag_name":"v2.0.0","assets":[{msi}]}}"#),
            false,
            "linux release with only a Windows asset",
        ),
        (
            format!(r#"{{"tag_name":"v2.0.0","assets":[{appimage}]}}"#),
            true,
            "windows release with only a Linux asset",
        ),
        (
            format!(r#"{{"tag_name":"v2.0.0","assets":[{text}]}}"#),
            false,
            "only unsupported assets",
        ),
    ];

    for (body, is_windows, name) in cases {
        assert_check_none_for_release(body, is_windows, name).await;
    }
}

#[tokio::test]
async fn check_release_notes_follow_body_name_and_default_precedence() {
    let cases = [
        (
            "body wins over name",
            r#"{"tag_name":"v2.0.0","body":"BODY","name":"NAME","unknown_field":{"nested":true},"assets":[{"name":"EmuSync-x86_64.AppImage","browser_download_url":"https://example.com/download/EmuSync-x86_64.AppImage"}]}"#,
            "BODY",
            0,
        ),
        (
            "name used when body is null",
            r#"{"tag_name":"v2.0.0","body":null,"name":"NAME","assets":[{"name":"EmuSync-x86_64.AppImage","browser_download_url":"https://example.com/download/EmuSync-x86_64.AppImage","size":777}]}"#,
            "NAME",
            777,
        ),
        (
            "default notes when body and name are absent",
            r#"{"tag_name":"v2.0.0","assets":[{"name":"EmuSync-x86_64.AppImage","browser_download_url":"https://example.com/download/EmuSync-x86_64.AppImage","size":888}]}"#,
            "New version 2.0.0 available.",
            888,
        ),
        (
            "empty body is distinct from an absent body",
            r#"{"tag_name":"v2.0.0","body":"","name":"NAME","assets":[{"name":"EmuSync-x86_64.AppImage","browser_download_url":"https://example.com/download/EmuSync-x86_64.AppImage","size":999}]}"#,
            "",
            999,
        ),
    ];

    for (name, body, expected_notes, expected_size) in cases {
        let server = TestServer::bind();
        let base_url = server.base_url();
        let server_task = server.spawn(move |mut connection| async move {
            connection
                .write_all(&response_with_content_length("200 OK", body.as_bytes()))
                .await?;
            Ok(())
        });
        let source = GitHubUpdateSource::new(client(), "owner", "repo", "1.0.0", false)
            .with_base_url(base_url);

        let update = source
            .check()
            .await
            .unwrap_or_else(|error| panic!("{name}: check failed: {error}"))
            .unwrap_or_else(|| panic!("{name}: expected an update"));
        assert_eq!(update.release_notes, expected_notes, "{name}");
        assert_eq!(update.asset_size, expected_size, "{name}");
        assert_eq!(update.version, "2.0.0", "{name}");
        assert_eq!(update.tag_name, "v2.0.0", "{name}");
        assert_eq!(update.asset_name, "EmuSync-x86_64.AppImage", "{name}");
        assert_eq!(
            update.download_url, "https://example.com/download/EmuSync-x86_64.AppImage",
            "{name}"
        );

        let requests = server_task.finish().await;
        assert_eq!(requests.len(), 1, "{name}");
        assert_eq!(requests[0].target, RELEASE_ENDPOINT, "{name}");
    }
}

#[tokio::test]
async fn check_rejects_non_200_success_statuses_like_kotlin() {
    let body = r#"{"tag_name":"v2.0.0","assets":[{"name":"EmuSync-x86_64.AppImage","browser_download_url":"https://example.com/download/EmuSync-x86_64.AppImage","size":1024}]}"#;
    let mut failures = Vec::new();

    for status_line in ["201 Created", "202 Accepted"] {
        let server = TestServer::bind();
        let base_url = server.base_url();
        let server_task = server.spawn(move |mut connection| async move {
            connection
                .write_all(&response_with_content_length(status_line, body.as_bytes()))
                .await?;
            Ok(())
        });
        let source = GitHubUpdateSource::new(client(), "owner", "repo", "1.0.0", false)
            .with_base_url(base_url);
        let result = source.check().await;
        if !matches!(result, Ok(None)) {
            failures.push(format!(
                "HTTP {status_line}: expected Kotlin-compatible Ok(None), got {result:?}"
            ));
        }
        let requests = server_task.finish().await;
        assert_eq!(requests.len(), 1, "HTTP {status_line}");
        assert_eq!(requests[0].target, RELEASE_ENDPOINT, "HTTP {status_line}");
    }

    assert!(
        failures.is_empty(),
        "known Kotlin parity regression:\n{}",
        failures.join("\n")
    );
}

#[tokio::test]
async fn check_tag_prefix_normalization_matches_kotlin() {
    let cases = [
        ("v2.0.0", Some("2.0.0")),
        ("V2.0.0", Some("2.0.0")),
        ("vV2.0.0", Some("2.0.0")),
        ("vv2.0.0", None),
        ("Vv2.0.0", None),
    ];
    let mut failures = Vec::new();

    for (tag, expected_version) in cases {
        let body = format!(
            r#"{{"tag_name":"{tag}","assets":[{{"name":"EmuSync-x86_64.AppImage","browser_download_url":"https://example.com/download/EmuSync-x86_64.AppImage","size":1024}}]}}"#
        );
        let server = TestServer::bind();
        let base_url = server.base_url();
        let server_task = server.spawn(move |mut connection| async move {
            connection
                .write_all(&response_with_content_length("200 OK", body.as_bytes()))
                .await?;
            Ok(())
        });
        let source = GitHubUpdateSource::new(client(), "owner", "repo", "1.0.0", false)
            .with_base_url(base_url);
        let result = source.check().await;

        match (expected_version, result) {
            (Some(expected_version), Ok(Some(update))) => {
                if update.version != expected_version {
                    failures.push(format!(
                        "tag {tag:?}: expected version {expected_version:?}, got {:?}",
                        update.version
                    ));
                }
                if update.tag_name != tag {
                    failures.push(format!(
                        "tag {tag:?}: expected original tag_name, got {:?}",
                        update.tag_name
                    ));
                }
            }
            (None, Ok(None)) => {}
            (Some(expected_version), other) => failures.push(format!(
                "tag {tag:?}: expected Some version {expected_version:?}, got {other:?}"
            )),
            (None, other) => failures.push(format!(
                "tag {tag:?}: expected Kotlin-compatible None, got {other:?}"
            )),
        }

        let requests = server_task.finish().await;
        assert_eq!(requests.len(), 1, "tag {tag:?}");
        assert_eq!(requests[0].target, RELEASE_ENDPOINT, "tag {tag:?}");
    }

    assert!(
        failures.is_empty(),
        "known Kotlin parity regression:\n{}",
        failures.join("\n")
    );
}

#[tokio::test]
async fn source_download_delegates_bytes_errors_and_progress() {
    let cases = [
        (
            "close-delimited success",
            http_response("200 OK", &[("Connection", "close")], b"UPDATE"),
            ExpectedDownload::Success(b"UPDATE"),
        ),
        (
            "http failure",
            http_response(
                "503 Service Unavailable",
                &[("Content-Length", "0"), ("Connection", "close")],
                b"",
            ),
            ExpectedDownload::Http("503"),
        ),
        (
            "empty download",
            http_response(
                "200 OK",
                &[("Content-Length", "0"), ("Connection", "close")],
                b"",
            ),
            ExpectedDownload::Empty,
        ),
    ];

    for (name, response, expected) in cases {
        let dir = tempdir().expect("create update tempdir");
        let destination = dir.path().join("update.AppImage");
        std::fs::write(&destination, b"PREVIOUS").expect("write previous update");

        let server = TestServer::bind();
        let download_url = server.url("/asset");
        let server_task = server.spawn(move |mut connection| async move {
            connection.write_all(&response).await?;
            Ok(())
        });

        let info = UpdateInfo {
            version: "2.0.0".into(),
            tag_name: "v2.0.0".into(),
            release_notes: "notes".into(),
            download_url: download_url.clone(),
            asset_size: 0,
            asset_name: "EmuSync-x86_64.AppImage".into(),
        };
        let source = GitHubUpdateSource::new(client(), "owner", "repo", "1.0.0", false);
        let progress = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let progress_clone = progress.clone();
        let destination_string = destination.to_string_lossy().into_owned();
        let result = source
            .download(&info, &destination_string, move |value| {
                progress_clone.lock().expect("lock progress").push(value);
            })
            .await;

        match expected {
            ExpectedDownload::Success(bytes) => {
                result.unwrap_or_else(|error| panic!("{name}: expected success: {error}"));
                assert_eq!(
                    std::fs::read(&destination).expect("read destination"),
                    bytes,
                    "{name}"
                );
                assert_eq!(
                    *progress.lock().expect("lock progress"),
                    vec![1.0],
                    "{name}"
                );
            }
            ExpectedDownload::Http(status) => {
                match result {
                    Err(UpdateError::Http(message)) if message.contains(status) => {}
                    other => panic!("{name}: expected HTTP {status}, got {other:?}"),
                }
                assert_eq!(
                    std::fs::read(&destination).expect("read destination"),
                    b"PREVIOUS",
                    "{name}"
                );
                assert!(
                    progress.lock().expect("lock progress").is_empty(),
                    "{name}: failed HTTP responses must not report progress"
                );
            }
            ExpectedDownload::Empty => {
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
            }
        }

        assert_directory_entries(dir.path(), &["update.AppImage"]);
        let requests = server_task.finish().await;
        assert_eq!(requests.len(), 1, "{name}");
        assert_eq!(requests[0].method, "GET", "{name}");
        assert_eq!(requests[0].target, "/asset", "{name}");
        assert_eq!(
            requests[0].header("User-Agent"),
            Some("EmuSync-Desktop-App"),
            "{name}"
        );
    }
}

enum ExpectedDownload {
    Success(&'static [u8]),
    Http(&'static str),
    Empty,
}
