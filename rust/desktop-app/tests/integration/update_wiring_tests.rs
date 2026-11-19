//! Actualizador conectado: el botón «Check Updates» consulta las releases, descarga el
//! AppImage y deja la actualización lista; las compilaciones de desarrollo nunca
//! contactan GitHub por su cuenta.

use crate::common;

use common::{read_http_request_async, write_http_json_async};
use emusync_application::UpdateState;
use emusync_data_local::ConfigManager;
use emusync_desktop_app::{AppEvent, DesktopCoordinator, coordinator::UpdateSourceConfig};
use emusync_domain::{config::Language, update::UpdateInfo};
use emusync_presentation::actions::UiAction;
use std::{
    sync::{Arc, mpsc::Receiver},
    time::Duration,
};
use tempfile::tempdir;
use tokio::{io::AsyncWriteExt, net::TcpListener};

const ASSET: &[u8] = b"new EmuSync AppImage bytes";

/// The asset this platform installs from a release that publishes both packages.
const PLATFORM_ASSET: &str = if cfg!(windows) {
    "EmuSync-x64.msi"
} else {
    "EmuSync-x86_64.AppImage"
};

/// Serves GitHub's latest-release API with the Linux and Windows packages, and the assets.
async fn release_server(latest_tag: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let download = format!("{base}/download");
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let download = download.clone();
            tokio::spawn(async move {
                let Ok((headers, _)) = read_http_request_async(&mut stream).await else {
                    return;
                };
                if headers.starts_with("GET /repos/ThEeLxD192/emu-sync/releases/latest ") {
                    let body = format!(
                        r#"{{"tag_name":"{latest_tag}","name":"EmuSync {latest_tag}","body":"Fixes","assets":[{{"name":"EmuSync-x86_64.AppImage","browser_download_url":"{download}/EmuSync-x86_64.AppImage","size":{size}}},{{"name":"EmuSync-x64.msi","browser_download_url":"{download}/EmuSync-x64.msi","size":{size}}}]}}"#,
                        size = ASSET.len()
                    );
                    let _ = write_http_json_async(&mut stream, 200, "OK", &body).await;
                } else if headers.starts_with("GET /download/EmuSync-x86_64.AppImage ")
                    || headers.starts_with("GET /download/EmuSync-x64.msi ")
                {
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        ASSET.len()
                    );
                    let _ = stream.write_all(head.as_bytes()).await;
                    let _ = stream.write_all(ASSET).await;
                    let _ = stream.flush().await;
                } else {
                    let _ = write_http_json_async(&mut stream, 404, "Not Found", "{}").await;
                }
            });
        }
    });
    base
}

fn coordinator(
    source: UpdateSourceConfig,
) -> (DesktopCoordinator, Receiver<AppEvent>, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let coordinator = DesktopCoordinator::new(
        manager,
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    )
    .with_update_source(source);
    (coordinator, rx, dir)
}

async fn next_matching(
    rx: &Receiver<AppEvent>,
    mut predicate: impl FnMut(&AppEvent) -> bool,
) -> AppEvent {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            while let Ok(event) = rx.try_recv() {
                if predicate(&event) {
                    return event;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("timed out waiting for the event")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_finds_the_newer_release_and_downloads_it_ready_to_restart() {
    let base = release_server("v99.0.0").await;
    let (coordinator, rx, _dir) = coordinator(UpdateSourceConfig::Controlled(base));

    coordinator.handle_action(UiAction::CheckForUpdates { manual: true });
    let AppEvent::UpdateStateChanged(UpdateState::Available(info)) = next_matching(&rx, |e| {
        matches!(e, AppEvent::UpdateStateChanged(UpdateState::Available(_)))
    })
    .await
    else {
        unreachable!()
    };
    assert_eq!(info.version, "99.0.0");
    // Each platform picks its own package from the same release.
    assert_eq!(info.asset_name, PLATFORM_ASSET);

    coordinator.handle_action(UiAction::DownloadUpdate(info));
    let AppEvent::UpdateStateChanged(UpdateState::ReadyToRestart {
        downloaded_path, ..
    }) = next_matching(&rx, |e| {
        matches!(
            e,
            AppEvent::UpdateStateChanged(
                UpdateState::ReadyToRestart { .. } | UpdateState::Error(_)
            )
        )
    })
    .await
    else {
        panic!("the download must finish ready to restart");
    };
    let downloaded = std::fs::read(&downloaded_path).unwrap();
    std::fs::remove_file(&downloaded_path).unwrap();
    assert_eq!(downloaded, ASSET);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_manual_check_on_the_latest_version_says_so() {
    let base = release_server(concat!("v", env!("CARGO_PKG_VERSION"))).await;
    let (coordinator, rx, _dir) = coordinator(UpdateSourceConfig::Controlled(base));

    coordinator.handle_action(UiAction::CheckForUpdates { manual: true });

    next_matching(
        &rx,
        |e| matches!(e, AppEvent::Toast(m) if m == "EmuSync is up to date."),
    )
    .await;
}

#[tokio::test]
async fn development_builds_without_a_controlled_server_never_check() {
    let (coordinator, rx, _dir) = coordinator(UpdateSourceConfig::Disabled);

    coordinator.handle_action(UiAction::CheckForUpdates { manual: false });
    coordinator.handle_action(UiAction::CheckForUpdates { manual: true });

    let events: Vec<_> = rx.try_iter().collect();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AppEvent::UpdateStateChanged(_))),
        "{events:?}"
    );
    assert_eq!(
        events
            .iter()
            .filter(
                |e| matches!(e, AppEvent::Toast(m) if m.contains("disabled in development builds"))
            )
            .count(),
        1,
        "only the manual check explains it: {events:?}"
    );
}

#[tokio::test]
async fn a_portable_copy_never_checks_and_explains_that_it_replaces_itself_by_hand() {
    let (coordinator, rx, _dir) = coordinator(UpdateSourceConfig::Portable);

    coordinator.handle_action(UiAction::CheckForUpdates { manual: false });
    coordinator.handle_action(UiAction::CheckForUpdates { manual: true });

    let events: Vec<_> = rx.try_iter().collect();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AppEvent::UpdateStateChanged(_))),
        "{events:?}"
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, AppEvent::Toast(m) if m.contains("portable copy")))
            .count(),
        1,
        "only the manual check explains it: {events:?}"
    );
}

#[test]
fn only_development_builds_honour_a_controlled_release_server() {
    let url = Some("http://127.0.0.1:9".to_string());
    assert_eq!(
        UpdateSourceConfig::select(url.clone(), false),
        UpdateSourceConfig::GitHub
    );
    assert_eq!(
        UpdateSourceConfig::select(url, true),
        UpdateSourceConfig::Controlled("http://127.0.0.1:9".into())
    );
    assert_eq!(
        UpdateSourceConfig::select(Some("  ".into()), true),
        UpdateSourceConfig::Disabled
    );
    assert_eq!(
        UpdateSourceConfig::select(None, false),
        UpdateSourceConfig::GitHub
    );
}

/// How a controlled release server answers the latest-release request.
#[derive(Clone, Copy)]
enum ReleaseAnswer {
    /// A complete response with this status line and an empty body.
    Status(&'static str),
    /// `200 OK` with a body that is not a release.
    NotARelease,
    /// Reads the request and closes the connection without answering.
    CloseWithoutAnswer,
}

/// Serves the latest-release endpoint with `answer` on every request.
async fn release_server_answering(answer: ReleaseAnswer) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                if read_http_request_async(&mut stream).await.is_err() {
                    return;
                }
                let response = match answer {
                    ReleaseAnswer::Status(status_line) => format!(
                        "HTTP/1.1 {status_line}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    ),
                    ReleaseAnswer::NotARelease => {
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 8\r\nConnection: close\r\n\r\nnot json"
                            .to_string()
                    }
                    ReleaseAnswer::CloseWithoutAnswer => return,
                };
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.flush().await;
            });
        }
    });
    base
}

/// Waits for an event matching `predicate`, keeping every event received meanwhile in
/// `seen` so the test can also check what was not sent.
async fn collect_until(
    rx: &Receiver<AppEvent>,
    seen: &mut Vec<AppEvent>,
    mut predicate: impl FnMut(&AppEvent) -> bool,
) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            while let Ok(event) = rx.try_recv() {
                let is_match = predicate(&event);
                seen.push(event);
                if is_match {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for the event; received {seen:?}"));
}

fn toasts(events: &[AppEvent]) -> Vec<&str> {
    events
        .iter()
        .filter_map(|event| match event {
            AppEvent::Toast(message) => Some(message.as_str()),
            _ => None,
        })
        .collect()
}

/// The detail of the first failed check among `events`.
fn check_failed_detail(events: &[AppEvent]) -> &str {
    events
        .iter()
        .find_map(|event| match event {
            AppEvent::UpdateStateChanged(UpdateState::CheckFailed(detail)) => Some(detail.as_str()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no failed check was published: {events:?}"))
}

fn is_failed_check_toast(event: &AppEvent) -> bool {
    matches!(
        event,
        AppEvent::Toast(message)
            if message.starts_with("Could not check for updates: ")
                || message.starts_with("No se pudo comprobar si hay actualizaciones: ")
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_manual_check_against_an_unavailable_server_says_it_could_not_check_never_up_to_date() {
    let base = release_server_answering(ReleaseAnswer::Status("503 Service Unavailable")).await;
    let (coordinator, rx, _dir) = coordinator(UpdateSourceConfig::Controlled(base));

    coordinator.handle_action(UiAction::CheckForUpdates { manual: true });

    let mut seen = Vec::new();
    collect_until(&rx, &mut seen, is_failed_check_toast).await;
    assert_eq!(
        check_failed_detail(&seen),
        "HTTP error: 503 Service Unavailable",
        "the detail names the server's answer once: {seen:?}"
    );
    assert_eq!(
        toasts(&seen),
        vec!["Could not check for updates: HTTP error: 503 Service Unavailable"],
        "{seen:?}"
    );
    assert!(
        !seen.iter().any(|event| matches!(
            event,
            AppEvent::UpdateStateChanged(
                UpdateState::Idle | UpdateState::Available(_) | UpdateState::Error(_)
            )
        )),
        "a failed check is neither \"no update\", a release nor an update error: {seen:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_manual_check_in_spanish_reports_the_failure_in_spanish() {
    let base = release_server_answering(ReleaseAnswer::Status("503 Service Unavailable")).await;
    let (coordinator, rx, _dir) = coordinator(UpdateSourceConfig::Controlled(base));
    coordinator.language.set(Language::Spanish);

    coordinator.handle_action(UiAction::CheckForUpdates { manual: true });

    let mut seen = Vec::new();
    collect_until(&rx, &mut seen, is_failed_check_toast).await;
    let detail = check_failed_detail(&seen);
    assert_eq!(
        toasts(&seen),
        vec![format!("No se pudo comprobar si hay actualizaciones: {detail}").as_str()],
        "{seen:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_manual_check_whose_connection_closes_or_whose_answer_is_not_a_release_reports_the_failure()
 {
    for answer in [
        ReleaseAnswer::CloseWithoutAnswer,
        ReleaseAnswer::NotARelease,
    ] {
        let base = release_server_answering(answer).await;
        let (coordinator, rx, _dir) = coordinator(UpdateSourceConfig::Controlled(base.clone()));

        coordinator.handle_action(UiAction::CheckForUpdates { manual: true });

        let mut seen = Vec::new();
        collect_until(&rx, &mut seen, |event| matches!(event, AppEvent::Toast(_))).await;
        let detail = check_failed_detail(&seen);
        assert!(!detail.is_empty(), "{base}: the failure has a detail");
        assert_eq!(
            toasts(&seen),
            vec![format!("Could not check for updates: {detail}").as_str()],
            "{base}: {seen:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_manual_check_against_a_repository_without_releases_says_it_is_up_to_date() {
    let base = release_server_answering(ReleaseAnswer::Status("404 Not Found")).await;
    let (coordinator, rx, _dir) = coordinator(UpdateSourceConfig::Controlled(base));
    coordinator.language.set(Language::Spanish);

    coordinator.handle_action(UiAction::CheckForUpdates { manual: true });

    let mut seen = Vec::new();
    collect_until(&rx, &mut seen, |event| matches!(event, AppEvent::Toast(_))).await;
    assert_eq!(toasts(&seen), vec!["EmuSync está al día."], "{seen:?}");
}

// A single-threaded runtime makes the order deterministic: the check task publishes
// its state and runs its completion (where a toast would be sent) without yielding, so
// once the test sees the published state the automatic check has finished entirely.
#[tokio::test]
async fn a_failed_automatic_check_stays_silent_and_a_later_manual_one_reports_it() {
    let base = release_server_answering(ReleaseAnswer::Status("503 Service Unavailable")).await;
    let (coordinator, rx, _dir) = coordinator(UpdateSourceConfig::Controlled(base));

    coordinator.handle_action(UiAction::CheckForUpdates { manual: false });
    let mut seen = Vec::new();
    collect_until(&rx, &mut seen, |event| {
        matches!(
            event,
            AppEvent::UpdateStateChanged(UpdateState::CheckFailed(_))
        )
    })
    .await;
    seen.extend(rx.try_iter());
    assert_eq!(
        toasts(&seen),
        Vec::<&str>::new(),
        "the automatic check must not speak: {seen:?}"
    );

    coordinator.handle_action(UiAction::CheckForUpdates { manual: true });
    collect_until(&rx, &mut seen, |event| matches!(event, AppEvent::Toast(_))).await;
    let detail = check_failed_detail(&seen);
    assert_eq!(
        toasts(&seen),
        vec![format!("Could not check for updates: {detail}").as_str()],
        "only the manual check reports the failure: {seen:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_download_the_server_refuses_shows_its_status_once_in_the_update_error() {
    let cases = [
        (
            "503 Service Unavailable",
            "Failed to download update: HTTP error: 503 Service Unavailable",
        ),
        (
            "503 Down for maintenance",
            "Failed to download update: HTTP error: 503 Service Unavailable",
        ),
        (
            "599 Network Connect Timeout Error",
            "Failed to download update: HTTP error: 599 <unknown status code>",
        ),
    ];

    for (status_line, expected) in cases {
        let base = release_server_answering(ReleaseAnswer::Status(status_line)).await;
        let (coordinator, rx, _dir) = coordinator(UpdateSourceConfig::Controlled(base.clone()));
        let info = UpdateInfo {
            version: "99.0.0".into(),
            tag_name: "v99.0.0".into(),
            release_notes: "Fixes".into(),
            download_url: format!("{base}/download/{PLATFORM_ASSET}"),
            asset_size: ASSET.len() as i64,
            asset_name: PLATFORM_ASSET.into(),
        };

        coordinator.handle_action(UiAction::DownloadUpdate(info));

        let mut seen = Vec::new();
        collect_until(&rx, &mut seen, |event| {
            matches!(
                event,
                AppEvent::UpdateStateChanged(
                    UpdateState::Error(_) | UpdateState::ReadyToRestart { .. }
                )
            )
        })
        .await;
        match seen.last() {
            Some(AppEvent::UpdateStateChanged(UpdateState::Error(message))) => {
                assert_eq!(message, expected, "{status_line}: {seen:?}");
            }
            other => panic!("{status_line}: a refused download is an update error, got {other:?}"),
        }
    }
}
