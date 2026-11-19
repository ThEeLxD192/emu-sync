//! Redirects from HTTPS to plain HTTP: a release build refuses them before connecting,
//! while the development policy follows the same redirect.

use crate::common;

use common::{TestServer, assert_directory_entries, response_with_content_length};
use emusync_data_updates::{
    ReleaseTransport, UpdateDownloader, UpdateError, release_client_builder,
};
use rustls::pki_types::PrivatePkcs8KeyDer;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Instant;
use tempfile::tempdir;

const ASSET_BYTES: &[u8] = b"new EmuSync AppImage bytes";

/// A loopback HTTPS server with a certificate generated for one test. It answers one
/// connection with `response` and reports the request target it received.
struct TlsServer {
    base_url: String,
    certificate: reqwest::Certificate,
    thread: JoinHandle<String>,
}

impl TlsServer {
    fn answering(response: Vec<u8>) -> Self {
        let generated = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_owned()])
            .expect("generate a synthetic loopback certificate");
        let certificate_der = generated.cert.der().clone();
        let private_key = PrivatePkcs8KeyDer::from(generated.signing_key.serialize_der());
        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![certificate_der.clone()], private_key.into())
            .expect("configure the TLS server");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind TLS loopback listener");
        listener
            .set_nonblocking(true)
            .expect("set TLS listener nonblocking");
        let base_url = format!(
            "https://{}",
            listener.local_addr().expect("read TLS listener address")
        );
        let thread = thread::spawn(move || {
            let stream = accept_within_harness_limit(&listener);
            stream
                .set_nonblocking(false)
                .expect("set TLS stream blocking");
            stream
                .set_read_timeout(Some(common::HARNESS_WAIT_LIMIT))
                .expect("bound TLS reads");
            let connection =
                rustls::ServerConnection::new(Arc::new(server_config)).expect("start TLS session");
            let mut tls = rustls::StreamOwned::new(connection, stream);
            let target = read_request_target(&mut tls);
            tls.write_all(&response).expect("write TLS response");
            tls.flush().expect("flush TLS response");
            tls.conn.send_close_notify();
            let _ = tls.flush();
            target
        });
        Self {
            base_url,
            certificate: reqwest::Certificate::from_der(certificate_der.as_ref())
                .expect("load synthetic certificate"),
            thread,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    /// The request target the server received.
    fn finish(self) -> String {
        self.thread.join().expect("TLS server thread")
    }
}

fn accept_within_harness_limit(listener: &TcpListener) -> std::net::TcpStream {
    let deadline = Instant::now() + common::HARNESS_WAIT_LIMIT;
    loop {
        match listener.accept() {
            Ok((stream, _)) => return stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "the client never connected to the TLS server"
                );
                thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(error) => panic!("accept TLS connection: {error}"),
        }
    }
}

fn read_request_target(tls: &mut impl Read) -> String {
    let mut head = Vec::new();
    let mut byte = [0; 1];
    while !head.ends_with(b"\r\n\r\n") {
        let count = tls.read(&mut byte).expect("read TLS request");
        assert_eq!(count, 1, "the client closed before finishing its request");
        head.push(byte[0]);
    }
    let head = String::from_utf8(head).expect("request head is UTF-8");
    let request_line = head.lines().next().expect("request line");
    request_line
        .split_whitespace()
        .nth(1)
        .expect("request target")
        .to_owned()
}

fn redirect_to(location: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
    .into_bytes()
}

/// The production release policy, trusting the test certificate and ignoring any proxy
/// set in the environment.
fn release_client_trusting(
    transport: ReleaseTransport,
    certificate: &reqwest::Certificate,
) -> reqwest::Client {
    release_client_builder(transport)
        .add_root_certificate(certificate.clone())
        .no_proxy()
        .build()
        .expect("build release client")
}

#[tokio::test]
async fn https_only_download_refuses_a_redirect_from_https_to_plain_http() {
    let plain_server = TestServer::bind();
    let tls_server = TlsServer::answering(redirect_to(&plain_server.url("/asset")));
    let downloader = UpdateDownloader::new(release_client_trusting(
        ReleaseTransport::HttpsOnly,
        &tls_server.certificate,
    ));
    let dir = tempdir().expect("create update tempdir");
    let destination = dir.path().join("update.AppImage");

    let result = tokio::time::timeout(
        common::HARNESS_WAIT_LIMIT,
        downloader.download(&tls_server.url("/download"), &destination, |_| {}),
    )
    .await
    .expect("a refused redirect must not wait on the plain HTTP server");

    assert!(
        matches!(result, Err(UpdateError::Network(_))),
        "a redirect to plain HTTP must fail the download, got {result:?}"
    );
    assert_eq!(tls_server.finish(), "/download");
    plain_server.assert_no_connection();
    assert_directory_entries(dir.path(), &[]);
}

#[tokio::test]
async fn plain_http_policy_follows_the_same_redirect_from_https_to_plain_http() {
    let plain_server = TestServer::bind();
    let tls_server = TlsServer::answering(redirect_to(&plain_server.url("/asset")));
    let plain_task = plain_server.spawn(|mut connection| async move {
        connection
            .write_all(&response_with_content_length("200 OK", ASSET_BYTES))
            .await
    });
    let downloader = UpdateDownloader::new(release_client_trusting(
        ReleaseTransport::AllowPlainHttp,
        &tls_server.certificate,
    ));
    let dir = tempdir().expect("create update tempdir");
    let destination = dir.path().join("update.AppImage");

    tokio::time::timeout(
        common::HARNESS_WAIT_LIMIT,
        downloader.download(&tls_server.url("/download"), &destination, |_| {}),
    )
    .await
    .expect("redirected download finishes")
    .expect("the development policy follows a redirect to plain HTTP");

    assert_eq!(tls_server.finish(), "/download");
    let plain_requests = plain_task.finish().await;
    assert_eq!(plain_requests.len(), 1);
    assert_eq!(plain_requests[0].target, "/asset");
    assert_eq!(
        std::fs::read(&destination).expect("read downloaded update"),
        ASSET_BYTES
    );
}
