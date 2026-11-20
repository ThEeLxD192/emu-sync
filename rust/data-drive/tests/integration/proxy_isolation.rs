//! The tests of this binary talk to servers they start on this machine, so none of their
//! HTTP clients may take a proxy from the environment: with `HTTP_PROXY` set, those
//! requests would go to the proxy and the tests would fail or hang. The shared helpers in
//! `common` build every client without it.
//!
//! An environment proxy is process-wide state, so this check runs in a copy of this test
//! binary started with the proxy variables pointing at a loopback server that records
//! every request it receives.

use crate::common;

use emusync_data_drive::DriveHttpPolicy;
use reqwest::StatusCode;
use std::{
    io::Write,
    net::{SocketAddr, TcpListener, TcpStream},
    process::Command,
    thread,
    time::Duration,
};

/// Tells the child test that the parent started it behind the recording proxy.
const CHILD_MARKER_VARIABLE: &str = "EMUSYNC_DRIVE_PROXY_ISOLATION_CHILD";

/// Every variable reqwest reads a proxy from, in both spellings.
const PROXY_VARIABLES: [&str; 6] = [
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
];

/// Variables that would make reqwest skip the proxy on their own.
const PROXY_BYPASS_VARIABLES: [&str; 3] = ["NO_PROXY", "no_proxy", "REQUEST_METHOD"];

/// The request line the recording server treats as the end of the recording.
const STOP_RECORDING_LINE: &str = "EMUSYNC-STOP-RECORDING";

const PROXY_RESPONSE: &str =
    "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

const ORIGIN_RESPONSE: &str = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

/// The path the child asks for with a client that does read the environment, to show
/// that the proxy is active in the child.
const CONTROL_PATH: &str = "/through-the-environment-proxy";

/// A loopback HTTP server that answers every request with `response` and records its
/// request line, one connection at a time and in the order they were accepted.
struct RecordingServer {
    address: SocketAddr,
    worker: thread::JoinHandle<Vec<String>>,
}

impl RecordingServer {
    fn start(response: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = thread::spawn(move || {
            let mut request_lines = Vec::new();
            for stream in listener.incoming() {
                let mut stream = stream.expect("accept a connection");
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let (headers, _) = common::read_request(&mut stream);
                let request_line = headers.lines().next().unwrap_or_default().to_owned();
                if request_line == STOP_RECORDING_LINE {
                    break;
                }
                let _ = stream.write_all(response.as_bytes());
                request_lines.push(request_line);
            }
            request_lines
        });
        Self { address, worker }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }

    /// Stops recording and returns the request lines received. Connections are accepted
    /// in order, so every one opened before this call is in the result.
    fn finish(self) -> Vec<String> {
        let mut stop = TcpStream::connect(self.address).unwrap();
        stop.write_all(format!("{STOP_RECORDING_LINE}\r\n\r\n").as_bytes())
            .unwrap();
        self.worker.join().unwrap()
    }
}

#[test]
fn shared_test_clients_reach_loopback_servers_without_the_environment_proxy() {
    let proxy = RecordingServer::start(PROXY_RESPONSE);
    let proxy_url = proxy.url("");
    // The child runs `direct_clients_child` by its full name in the test binary
    // (`proxy_isolation::...`).
    let module = module_path!().split_once("::").unwrap().1;
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            &format!("{module}::direct_clients_child"),
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_MARKER_VARIABLE, "1");
    for name in PROXY_BYPASS_VARIABLES {
        command.env_remove(name);
    }
    for name in PROXY_VARIABLES {
        command.env(name, &proxy_url);
    }
    let output = command.output().unwrap();
    let proxied = proxy.finish();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the child failed behind the proxy:\n{stdout}\n{stderr}"
    );
    assert!(
        stdout.contains("test result: ok. 1 passed;"),
        "the child did not run its test:\n{stdout}"
    );
    // Only the child's control request, made with a client that reads the environment.
    assert_eq!(proxied.len(), 1, "the proxy received: {proxied:?}");
    assert!(
        proxied[0].starts_with("GET http://127.0.0.1:")
            && proxied[0].ends_with(&format!("{CONTROL_PATH} HTTP/1.1")),
        "the proxy received an unexpected request: {}",
        proxied[0]
    );
}

/// Sends a request with each shared helper's client, in a process whose environment proxy
/// the parent test set.
#[tokio::test]
#[ignore = "re-executed as a child process with a proxy in its environment"]
async fn direct_clients_child() {
    if std::env::var_os(CHILD_MARKER_VARIABLE).is_none() {
        return;
    }
    let origin = RecordingServer::start(ORIGIN_RESPONSE);
    let policy_clients = common::direct_clients(DriveHttpPolicy::default());
    let requests = [
        (
            "common::direct_client",
            common::direct_client().get(origin.url("/direct-client")),
        ),
        (
            "metadata client of common::direct_clients",
            policy_clients.metadata.get(origin.url("/metadata")),
        ),
        (
            "transfer client of common::direct_clients",
            policy_clients.transfer.get(origin.url("/transfer")),
        ),
        (
            "upload client of common::direct_clients",
            policy_clients
                .upload
                .put(origin.url("/upload"))
                .body(vec![1, 2, 3]),
        ),
        (
            "empty upload client of common::direct_clients",
            policy_clients.empty_upload.put(origin.url("/empty-upload")),
        ),
    ];
    for (client_name, request) in requests {
        let status = request
            .send()
            .await
            .unwrap_or_else(|error| panic!("{client_name}: {error}"))
            .status();
        assert_eq!(
            status,
            StatusCode::OK,
            "{client_name} did not reach the loopback server; the proxy answers 502"
        );
    }

    let control_status = reqwest::Client::new()
        .get(origin.url(CONTROL_PATH))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        control_status,
        StatusCode::BAD_GATEWAY,
        "a client that reads the environment did not use the proxy, so this child proves nothing"
    );

    assert_eq!(
        origin.finish(),
        [
            "GET /direct-client HTTP/1.1",
            "GET /metadata HTTP/1.1",
            "GET /transfer HTTP/1.1",
            "PUT /upload HTTP/1.1",
            "PUT /empty-upload HTTP/1.1",
        ]
    );
}
