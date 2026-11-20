//! A release check that could not learn whether a newer version exists is an error, never
//! "up to date": transport failures are network errors that keep their cause, unexpected
//! statuses are HTTP errors, and a whole body that is not a release is invalid data.

use crate::common;

use common::{TestServer, assert_directory_entries, http_response, response_with_content_length};
use emusync_data_updates::{
    GitHubUpdateSource, ReleaseTransport, UpdateDownloader, UpdateError, release_client_builder,
};
use emusync_domain::update::{UpdateInfo, UpdateSource};
use std::net::SocketAddr;
use tempfile::tempdir;

const RELEASE_ENDPOINT: &str = "/repos/owner/repo/releases/latest";

/// The production release policy for a controlled plain HTTP server, kept away from any
/// proxy set in the environment so requests reach the local test server.
fn plain_http_client() -> reqwest::Client {
    release_client_builder(ReleaseTransport::AllowPlainHttp)
        .no_proxy()
        .build()
        .expect("build release client")
}

/// The result of a check, made by a client on version 1.0.0 for Linux, against a server
/// that answers the release request with `response` and then closes the connection.
async fn check_answered_with(
    name: &str,
    response: Vec<u8>,
) -> Result<Option<UpdateInfo>, UpdateError> {
    let server = TestServer::bind();
    let base_url = server.base_url();
    let server_task = server.spawn(move |mut connection| async move {
        connection.write_all(&response).await?;
        Ok(())
    });
    let source = GitHubUpdateSource::new(plain_http_client(), "owner", "repo", "1.0.0", false)
        .with_base_url(base_url);

    let result = tokio::time::timeout(common::HARNESS_WAIT_LIMIT, source.check())
        .await
        .unwrap_or_else(|_| panic!("{name}: the check must end once the server closes"));

    let requests = server_task.finish().await;
    assert_eq!(
        requests.len(),
        1,
        "{name}: expected only the release request, got {requests:?}"
    );
    assert_eq!(requests[0].target, RELEASE_ENDPOINT, "{name}");
    result
}

/// A loopback address that refuses connections, held by a socket that is bound but not
/// listening so no other test can take the port meanwhile.
struct RefusingAddress {
    _socket: tokio::net::TcpSocket,
    address: SocketAddr,
}

impl RefusingAddress {
    fn bind() -> Self {
        let socket = tokio::net::TcpSocket::new_v4().expect("create loopback socket");
        socket
            .bind("127.0.0.1:0".parse().expect("parse loopback address"))
            .expect("bind loopback socket without listening");
        let address = socket.local_addr().expect("read bound loopback address");
        Self {
            _socket: socket,
            address,
        }
    }

    /// The operating system's own description of the refusal, observed with a plain
    /// connection that does not go through the code under test.
    fn refusal_reported_by_the_system(&self) -> String {
        let error = std::net::TcpStream::connect_timeout(&self.address, common::HARNESS_WAIT_LIMIT)
            .expect_err("a bound socket that does not listen must refuse connections");
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::ConnectionRefused,
            "the test address must refuse connections, got {error}"
        );
        error.to_string()
    }
}

#[tokio::test]
async fn check_against_a_refused_connection_fails_naming_the_refusal() {
    let refusing = RefusingAddress::bind();
    let system_refusal = refusing.refusal_reported_by_the_system();
    let source = GitHubUpdateSource::new(plain_http_client(), "owner", "repo", "1.0.0", false)
        .with_base_url(format!("http://{}", refusing.address));

    let result = tokio::time::timeout(common::HARNESS_WAIT_LIMIT, source.check())
        .await
        .expect("a refused connection must end the check");

    match result {
        Err(error @ UpdateError::Network(_)) => {
            let shown = error.to_string();
            assert!(
                shown.contains(&system_refusal),
                "the error must name the refused connection ({system_refusal:?}), got: {shown}"
            );
        }
        other => panic!("a refused connection is a network error, got {other:?}"),
    }
}

#[tokio::test]
async fn download_against_a_refused_connection_fails_naming_the_refusal_and_keeps_the_previous_file()
 {
    let refusing = RefusingAddress::bind();
    let system_refusal = refusing.refusal_reported_by_the_system();
    let dir = tempdir().expect("create update tempdir");
    let destination = dir.path().join("update.AppImage");
    std::fs::write(&destination, b"PREVIOUS").expect("write previous update");
    let downloader = UpdateDownloader::new(plain_http_client());

    let result = tokio::time::timeout(
        common::HARNESS_WAIT_LIMIT,
        downloader.download(
            &format!("http://{}/asset", refusing.address),
            &destination,
            |_| {},
        ),
    )
    .await
    .expect("a refused connection must end the download");

    match result {
        Err(UpdateError::Network(message)) => assert!(
            message.contains(&system_refusal),
            "the error must name the refused connection ({system_refusal:?}), got: {message}"
        ),
        other => panic!("a refused download is a network error, got {other:?}"),
    }
    assert_eq!(
        std::fs::read(&destination).expect("read previous update"),
        b"PREVIOUS"
    );
    assert_directory_entries(dir.path(), &["update.AppImage"]);
}

#[tokio::test]
async fn check_against_a_server_that_does_not_speak_http_fails_as_a_network_error() {
    let result = check_answered_with(
        "non-HTTP answer",
        b"SSH-2.0-OpenSSH_9.6\r\nProtocol mismatch.\r\n".to_vec(),
    )
    .await;

    assert!(
        matches!(result, Err(UpdateError::Network(_))),
        "an answer that is not HTTP is a failed transfer, got {result:?}"
    );
}

#[tokio::test]
async fn check_whose_body_is_cut_short_fails_even_if_the_received_part_is_a_whole_older_release() {
    // The bytes that arrive are a complete release that is not newer: reading them as
    // "up to date" would hide that the transfer failed.
    let older_release = br#"{"tag_name":"v0.9.0","assets":[{"name":"EmuSync-x86_64.AppImage","browser_download_url":"https://example.com/EmuSync-x86_64.AppImage","size":1024}]}"#;
    let declared_length = (older_release.len() + 16).to_string();
    let cases = [
        (
            "whole older release with a longer declared length",
            http_response(
                "200 OK",
                &[
                    ("Content-Type", "application/json"),
                    ("Content-Length", declared_length.as_str()),
                    ("Connection", "close"),
                ],
                older_release,
            ),
        ),
        (
            "headers promising a body that never arrives",
            http_response(
                "200 OK",
                &[
                    ("Content-Type", "application/json"),
                    ("Content-Length", "64"),
                    ("Connection", "close"),
                ],
                b"",
            ),
        ),
        (
            "chunked body closed inside a chunk",
            http_response(
                "200 OK",
                &[
                    ("Content-Type", "application/json"),
                    ("Transfer-Encoding", "chunked"),
                    ("Connection", "close"),
                ],
                b"40\r\n{\"tag_name\":\"v0.9.0\"",
            ),
        ),
        (
            "chunked body closed before its last chunk",
            http_response(
                "200 OK",
                &[
                    ("Content-Type", "application/json"),
                    ("Transfer-Encoding", "chunked"),
                    ("Connection", "close"),
                ],
                b"15\r\n{\"tag_name\":\"v0.9.0\"}\r\n",
            ),
        ),
        (
            "chunked body with an invalid chunk size",
            http_response(
                "200 OK",
                &[
                    ("Content-Type", "application/json"),
                    ("Transfer-Encoding", "chunked"),
                    ("Connection", "close"),
                ],
                b"zz\r\n{\"tag_name\":\"v0.9.0\"}\r\n0\r\n\r\n",
            ),
        ),
    ];

    for (name, response) in cases {
        let result = check_answered_with(name, response).await;
        assert!(
            matches!(result, Err(UpdateError::Network(_))),
            "{name}: a body that did not arrive whole is a failed transfer, got {result:?}"
        );
    }
}

#[tokio::test]
async fn check_reads_a_whole_body_that_is_not_a_release_as_invalid_release_information() {
    let captive_portal: &[u8] = b"<!DOCTYPE html><html><head><title>Hotel Wi-Fi</title></head><body><form action=\"/login\">Accept the terms to continue</form></body></html>";
    let captive_portal_length = captive_portal.len().to_string();
    let cases = [
        (
            "captive portal login page",
            http_response(
                "200 OK",
                &[
                    ("Content-Type", "text/html; charset=utf-8"),
                    ("Content-Length", captive_portal_length.as_str()),
                    ("Connection", "close"),
                ],
                captive_portal,
            ),
        ),
        (
            "captive portal page delimited by the closed connection",
            http_response(
                "200 OK",
                &[("Content-Type", "text/html"), ("Connection", "close")],
                captive_portal,
            ),
        ),
        (
            "invalid JSON",
            response_with_content_length("200 OK", b"not json"),
        ),
        (
            "release without tag_name",
            response_with_content_length("200 OK", br#"{"name":"EmuSync","assets":[]}"#),
        ),
        ("empty body", response_with_content_length("200 OK", b"")),
        ("JSON null", response_with_content_length("200 OK", b"null")),
        (
            "JSON list of releases",
            response_with_content_length("200 OK", br#"[{"tag_name":"v2.0.0","assets":[]}]"#),
        ),
        (
            "tag_name that is not a string",
            response_with_content_length("200 OK", br#"{"tag_name":2,"assets":[]}"#),
        ),
        (
            "complete chunked body that is not JSON",
            http_response(
                "200 OK",
                &[("Transfer-Encoding", "chunked"), ("Connection", "close")],
                b"8\r\nnot json\r\n0\r\n\r\n",
            ),
        ),
    ];

    for (name, response) in cases {
        let result = check_answered_with(name, response).await;
        assert!(
            matches!(result, Err(UpdateError::InvalidRelease(_))),
            "{name}: a whole body that is not a release is invalid data, got {result:?}"
        );
    }
}

#[tokio::test]
async fn check_reports_each_unexpected_status_with_its_code_and_reason() {
    let release = br#"{"tag_name":"v2.0.0","assets":[{"name":"EmuSync-x86_64.AppImage","browser_download_url":"https://example.com/EmuSync-x86_64.AppImage","size":1024}]}"#;
    let cases = [
        (
            "201 with a release-shaped body",
            response_with_content_length("201 Created", release),
            "HTTP error: 201 Created",
        ),
        (
            "202 with a release-shaped body",
            response_with_content_length("202 Accepted", release),
            "HTTP error: 202 Accepted",
        ),
        (
            "204 No Content",
            http_response("204 No Content", &[("Connection", "close")], b""),
            "HTTP error: 204 No Content",
        ),
        (
            "301 without a Location",
            response_with_content_length("301 Moved Permanently", b""),
            "HTTP error: 301 Moved Permanently",
        ),
        (
            "304 Not Modified",
            http_response("304 Not Modified", &[("Connection", "close")], b""),
            "HTTP error: 304 Not Modified",
        ),
        (
            "401 with a release-shaped body",
            response_with_content_length("401 Unauthorized", release),
            "HTTP error: 401 Unauthorized",
        ),
        (
            "403 Forbidden (rate limit)",
            response_with_content_length("403 Forbidden", b""),
            "HTTP error: 403 Forbidden",
        ),
        (
            "410 Gone",
            response_with_content_length("410 Gone", b""),
            "HTTP error: 410 Gone",
        ),
        (
            "429 Too Many Requests",
            response_with_content_length("429 Too Many Requests", b""),
            "HTTP error: 429 Too Many Requests",
        ),
        (
            "500 with a release-shaped body",
            response_with_content_length("500 Internal Server Error", release),
            "HTTP error: 500 Internal Server Error",
        ),
        (
            "502 Bad Gateway",
            response_with_content_length("502 Bad Gateway", b"<html>Bad gateway</html>"),
            "HTTP error: 502 Bad Gateway",
        ),
        (
            "503 Service Unavailable",
            response_with_content_length("503 Service Unavailable", b""),
            "HTTP error: 503 Service Unavailable",
        ),
        (
            "599 without a standard reason",
            response_with_content_length("599 Network Connect Timeout Error", b""),
            "HTTP error: 599 <unknown status code>",
        ),
    ];

    for (name, response, expected) in cases {
        match check_answered_with(name, response).await {
            Err(error @ UpdateError::Http(_)) => assert_eq!(error.to_string(), expected, "{name}"),
            other => panic!("{name}: expected an HTTP error, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn check_of_a_repository_without_releases_ignores_the_body_of_the_404() {
    let github_not_found = br#"{"message":"Not Found","documentation_url":"https://docs.github.com/rest/releases/releases#get-the-latest-release","status":"404"}"#;
    let cases = [
        (
            "empty body",
            response_with_content_length("404 Not Found", b""),
        ),
        (
            "GitHub JSON body",
            response_with_content_length("404 Not Found", github_not_found),
        ),
        (
            "chunked HTML body",
            http_response(
                "404 Not Found",
                &[
                    ("Content-Type", "text/html"),
                    ("Transfer-Encoding", "chunked"),
                    ("Connection", "close"),
                ],
                b"9\r\nnot found\r\n0\r\n\r\n",
            ),
        ),
    ];

    for (name, response) in cases {
        let result = check_answered_with(name, response).await;
        assert!(
            matches!(result, Ok(None)),
            "{name}: a repository without releases has no update, got {result:?}"
        );
    }
}

#[tokio::test]
async fn check_names_an_unexpected_status_by_its_standard_reason_whatever_reason_the_server_writes()
{
    // The server's own reason phrase never reaches the user: a reason that repeats "HTTP",
    // names another status or is missing must still read as the code and its standard
    // reason, once, with nothing left over at the end.
    let cases = [
        (
            "503 with a custom reason",
            "503 Down for maintenance",
            "HTTP error: 503 Service Unavailable",
        ),
        (
            "503 without a reason",
            "503",
            "HTTP error: 503 Service Unavailable",
        ),
        (
            "500 whose reason names another status",
            "500 OK",
            "HTTP error: 500 Internal Server Error",
        ),
        (
            "502 whose reason already says HTTP",
            "502 HTTP 502 Bad Gateway",
            "HTTP error: 502 Bad Gateway",
        ),
        (
            "unassigned 599 without a reason",
            "599",
            "HTTP error: 599 <unknown status code>",
        ),
        (
            "highest three-digit status with a custom reason",
            "999 Custom Failure",
            "HTTP error: 999 <unknown status code>",
        ),
    ];

    for (name, status_line, expected) in cases {
        let response = response_with_content_length(status_line, b"");
        match check_answered_with(name, response).await {
            Err(error @ UpdateError::Http(_)) => assert_eq!(error.to_string(), expected, "{name}"),
            other => panic!("{name}: expected an HTTP error, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn check_classifies_the_answer_by_its_status_code_not_by_the_reason_the_server_writes() {
    let newer_release = common::signed_release::with_signed_files(
        r#"{"tag_name":"v2.0.0","assets":[{"name":"EmuSync-x86_64.AppImage","browser_download_url":"https://example.com/EmuSync-x86_64.AppImage","size":1024}]}"#,
    );
    let newer_release = newer_release.as_bytes();

    let not_found_saying_ok = check_answered_with(
        "404 whose reason says OK",
        response_with_content_length("404 OK", newer_release),
    )
    .await;
    assert!(
        matches!(not_found_saying_ok, Ok(None)),
        "a 404 is a repository without releases whatever its reason, got {not_found_saying_ok:?}"
    );

    let ok_saying_unavailable = check_answered_with(
        "200 whose reason says Service Unavailable",
        response_with_content_length("200 Service Unavailable", newer_release),
    )
    .await;
    match ok_saying_unavailable {
        Ok(Some(update)) => assert_eq!(update.version, "2.0.0"),
        other => {
            panic!("a 200 carrying a newer release offers it whatever its reason, got {other:?}")
        }
    }
}
