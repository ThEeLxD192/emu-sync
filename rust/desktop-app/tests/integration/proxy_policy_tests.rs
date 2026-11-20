//! Proxy policy of the Drive and release clients the coordinator builds.
//!
//! An environment proxy is process-wide state, so each test runs its scenario in a copy
//! of this test binary started with `HTTP_PROXY`, `HTTPS_PROXY` and `ALL_PROXY` pointing
//! at a loopback server that records every request it receives. With
//! `ProxyPolicy::Direct` that server must see nothing; with the default policy the
//! request must reach it instead of the real server.

use crate::common;

use emusync_application::UpdateState;
use emusync_data_local::ConfigManager;
use emusync_desktop_app::{
    AppEvent, DesktopCoordinator,
    coordinator::{ProxyPolicy, UpdateSourceConfig},
};
use emusync_domain::config::{AppConfig, ConfigurationStore, Credentials};
use std::{
    io::Write,
    net::{SocketAddr, TcpListener, TcpStream},
    process::Command,
    sync::{Arc, mpsc::Receiver},
    thread,
    time::Duration,
};
use tempfile::TempDir;
use tokio::sync::Notify;

/// Selects the scenario the child process runs.
const SCENARIO_VARIABLE: &str = "EMUSYNC_PROXY_POLICY_SCENARIO";

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

const TOKEN_FAILURE_RESPONSE: &str = "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: 24\r\nConnection: close\r\n\r\n{\"error\":\"server_error\"}";

const RELEASE_REQUEST_LINE: &str = "GET /repos/ThEeLxD192/emu-sync/releases/latest HTTP/1.1";

const TOKEN_REQUEST_LINE: &str = "POST /token HTTP/1.1";

/// Scenarios the child process can run.
const DIRECT_DRIVE_TOKEN: &str = "direct-drive-token";
const DEFAULT_DRIVE_TOKEN: &str = "default-drive-token";
const DIRECT_AFTER_DRIVE_CLIENTS_BUILT: &str = "direct-after-drive-clients-built";
const DIRECT_RELEASES_EITHER_ORDER: &str = "direct-releases-either-order";
const DEFAULT_RELEASES: &str = "default-releases";
const LAST_POLICY_RELEASES: &str = "last-policy-releases";

/// A loopback HTTP server that answers every request with `response` and records its
/// request line, one connection at a time and in the order they were accepted.
struct RecordingServer {
    address: SocketAddr,
    worker: thread::JoinHandle<Vec<String>>,
}

impl RecordingServer {
    fn start(response: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = thread::spawn(move || {
            let mut request_lines = Vec::new();
            for stream in listener.incoming() {
                let mut stream = stream.expect("accept a connection");
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let request_line = match common::read_http_request(&mut stream) {
                    Ok((headers, _)) => headers.lines().next().unwrap_or_default().to_owned(),
                    Err(error) => format!("unreadable request: {error}"),
                };
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

    fn url(&self) -> String {
        format!("http://{}", self.address)
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

/// Runs `scenario` in a child process whose environment proxy is a recording server,
/// and returns the request lines that proxy received.
fn run_scenario_behind_a_proxy(scenario: &str) -> Vec<String> {
    let proxy = RecordingServer::start(PROXY_RESPONSE.to_owned());
    let proxy_url = proxy.url();
    // The child runs `proxy_policy_child` by its full name in the test binary
    // (`proxy_policy_tests::...`).
    let module = module_path!().split_once("::").unwrap().1;
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            &format!("{module}::proxy_policy_child"),
            "--ignored",
            "--nocapture",
        ])
        .env(SCENARIO_VARIABLE, scenario)
        .env_remove("EMUSYNC_UPDATE_BASE_URL");
    for name in PROXY_BYPASS_VARIABLES {
        command.env_remove(name);
    }
    for name in PROXY_VARIABLES {
        command.env(name, &proxy_url);
    }
    let output = command.output().unwrap();
    let request_lines = proxy.finish();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the child failed in scenario {scenario}:\n{stdout}\n{stderr}"
    );
    assert!(
        stdout.contains("test result: ok. 1 passed;"),
        "the child did not run scenario {scenario}:\n{stdout}"
    );
    request_lines
}

fn assert_all_reached_the_proxy(request_lines: &[String], path: &str) {
    assert!(
        !request_lines.is_empty(),
        "the environment proxy received nothing"
    );
    for request_line in request_lines {
        assert!(
            request_line.contains(" http://127.0.0.1:") && request_line.contains(path),
            "the proxy received an unexpected request: {request_line}"
        );
    }
}

#[test]
fn direct_policy_refreshes_the_drive_token_without_passing_through_the_environment_proxy() {
    let proxied = run_scenario_behind_a_proxy(DIRECT_DRIVE_TOKEN);
    assert_eq!(proxied, Vec::<String>::new());
}

#[test]
fn default_policy_sends_the_drive_token_refresh_through_the_environment_proxy() {
    let proxied = run_scenario_behind_a_proxy(DEFAULT_DRIVE_TOKEN);
    assert_all_reached_the_proxy(&proxied, "/token HTTP/1.1");
}

#[test]
fn switching_to_direct_after_the_drive_clients_were_built_bypasses_the_environment_proxy() {
    let proxied = run_scenario_behind_a_proxy(DIRECT_AFTER_DRIVE_CLIENTS_BUILT);
    assert_eq!(proxied, Vec::<String>::new());
}

#[test]
fn direct_policy_checks_releases_without_the_environment_proxy_in_either_builder_order() {
    let proxied = run_scenario_behind_a_proxy(DIRECT_RELEASES_EITHER_ORDER);
    assert_eq!(proxied, Vec::<String>::new());
}

#[test]
fn default_policy_checks_releases_through_the_environment_proxy() {
    let proxied = run_scenario_behind_a_proxy(DEFAULT_RELEASES);
    assert_all_reached_the_proxy(&proxied, "/repos/ThEeLxD192/emu-sync/releases/latest ");
}

#[test]
fn the_last_proxy_policy_set_decides_how_releases_are_checked() {
    let proxied = run_scenario_behind_a_proxy(LAST_POLICY_RELEASES);
    assert_all_reached_the_proxy(&proxied, "/repos/ThEeLxD192/emu-sync/releases/latest ");
}

/// Runs the scenario named in `EMUSYNC_PROXY_POLICY_SCENARIO`, in a process whose
/// environment proxy the parent test set.
#[test]
#[ignore = "re-executed as a child process with a proxy in its environment"]
fn proxy_policy_child() {
    let Some(scenario) = std::env::var_os(SCENARIO_VARIABLE) else {
        return;
    };
    let scenario = scenario.into_string().unwrap();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async move {
            match scenario.as_str() {
                DIRECT_DRIVE_TOKEN => {
                    let received = refresh_drive_token(Some(ProxyPolicy::Direct), false).await;
                    assert_eq!(received, [TOKEN_REQUEST_LINE]);
                }
                DEFAULT_DRIVE_TOKEN => {
                    let received = refresh_drive_token(None, false).await;
                    assert_eq!(received, Vec::<String>::new());
                }
                DIRECT_AFTER_DRIVE_CLIENTS_BUILT => {
                    let received = refresh_drive_token(Some(ProxyPolicy::Direct), true).await;
                    assert_eq!(received, [TOKEN_REQUEST_LINE]);
                }
                DIRECT_RELEASES_EITHER_ORDER => check_releases_directly_in_either_order().await,
                DEFAULT_RELEASES => {
                    check_releases_expecting_the_proxy(|coordinator, source| {
                        coordinator.with_update_source(source)
                    })
                    .await;
                }
                LAST_POLICY_RELEASES => {
                    check_releases_expecting_the_proxy(|coordinator, source| {
                        coordinator
                            .with_proxy_policy(ProxyPolicy::Direct)
                            .with_update_source(source)
                            .with_proxy_policy(ProxyPolicy::FromEnvironment)
                    })
                    .await;
                }
                other => panic!("unknown scenario {other}"),
            }
        });
}

struct ChildCoordinator {
    coordinator: DesktopCoordinator,
    event_rx: Receiver<AppEvent>,
    redraw_notify: Arc<Notify>,
    _config_dir: TempDir,
}

/// A coordinator with the default proxy policy and a configuration connected to Drive.
fn connected_coordinator() -> ChildCoordinator {
    let config_dir = tempfile::tempdir().unwrap();
    let manager = ConfigManager::new(&config_dir.path().join("config.json")).unwrap();
    manager
        .save(&AppConfig {
            language: Default::default(),
            credentials: Some(Credentials {
                client_id: "client".into(),
                client_secret: "synthetic".into(),
                refresh_token: Some("refresh".into()),
            }),
            entries: vec![],
        })
        .unwrap();
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    let redraw_notify = Arc::new(Notify::new());
    let notify = redraw_notify.clone();
    let coordinator = DesktopCoordinator::new(
        manager,
        event_tx,
        Arc::new(move || notify.notify_one()),
        tokio::runtime::Handle::current(),
    );
    ChildCoordinator {
        coordinator,
        event_rx,
        redraw_notify,
        _config_dir: config_dir,
    }
}

/// Refreshes the Drive token against a recording token server and returns the request
/// lines it received. With `build_clients_first`, the Drive clients are built with the
/// default policy before `policy` is set.
async fn refresh_drive_token(
    policy: Option<ProxyPolicy>,
    build_clients_first: bool,
) -> Vec<String> {
    let token_server = RecordingServer::start(TOKEN_FAILURE_RESPONSE.to_owned());
    let child = connected_coordinator();
    let mut coordinator = child.coordinator;
    if build_clients_first {
        let config = coordinator.config_manager.load().unwrap();
        assert!(coordinator.build_check_orchestrator(&config).is_ok());
    }
    if let Some(policy) = policy {
        coordinator = coordinator.with_proxy_policy(policy);
    }
    let coordinator =
        coordinator.with_endpoints(common::make_drive_endpoints(token_server.address.port()));

    let result = coordinator.reconcile_persisted().await;

    assert!(
        result.is_err(),
        "the token refresh failed on both servers, so recovery cannot go on: {result:?}"
    );
    token_server.finish()
}

fn release_response() -> String {
    let body = r#"{"tag_name":"v99.0.0","name":"EmuSync v99.0.0","body":"Fixes","assets":[{"name":"EmuSync-x86_64.AppImage","browser_download_url":"http://127.0.0.1:9/EmuSync-x86_64.AppImage","size":4},{"name":"EmuSync-x64.msi","browser_download_url":"http://127.0.0.1:9/EmuSync-x64.msi","size":4}]}"#;
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Asks for a manual release check and returns how it ended.
async fn finished_release_check(child: &ChildCoordinator) -> UpdateState {
    child
        .coordinator
        .handle_action(emusync_presentation::actions::UiAction::CheckForUpdates { manual: true });
    let AppEvent::UpdateStateChanged(state) =
        common::recv_event_matching(&child.event_rx, &child.redraw_notify, |event| {
            matches!(
                event,
                AppEvent::UpdateStateChanged(
                    UpdateState::Available(_) | UpdateState::CheckFailed(_)
                )
            )
        })
        .await
    else {
        unreachable!()
    };
    state
}

async fn check_releases_directly_in_either_order() {
    let release_server = RecordingServer::start(release_response());
    let source = UpdateSourceConfig::Controlled(release_server.url());

    let mut policy_first = connected_coordinator();
    policy_first.coordinator = policy_first
        .coordinator
        .with_proxy_policy(ProxyPolicy::Direct)
        .with_update_source(source.clone());
    let mut source_first = connected_coordinator();
    source_first.coordinator = source_first
        .coordinator
        .with_update_source(source)
        .with_proxy_policy(ProxyPolicy::Direct);

    for (order, child) in [
        ("policy before source", &policy_first),
        ("source before policy", &source_first),
    ] {
        let state = finished_release_check(child).await;
        assert!(
            matches!(&state, UpdateState::Available(info) if info.version == "99.0.0"),
            "{order}: {state:?}"
        );
    }
    assert_eq!(
        release_server.finish(),
        [RELEASE_REQUEST_LINE, RELEASE_REQUEST_LINE]
    );
}

/// Checks releases with the coordinator `configure` returns, expecting the check to go to
/// the environment proxy, which answers 502, and never to the release server.
async fn check_releases_expecting_the_proxy(
    configure: impl FnOnce(DesktopCoordinator, UpdateSourceConfig) -> DesktopCoordinator,
) {
    let release_server = RecordingServer::start(release_response());
    let mut child = connected_coordinator();
    child.coordinator = configure(
        child.coordinator,
        UpdateSourceConfig::Controlled(release_server.url()),
    );

    let state = finished_release_check(&child).await;

    assert!(
        matches!(state, UpdateState::CheckFailed(_)),
        "the proxy answers 502, so the check must fail: {state:?}"
    );
    assert_eq!(release_server.finish(), Vec::<String>::new());
}
