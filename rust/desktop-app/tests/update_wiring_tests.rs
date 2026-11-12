//! Actualizador conectado: el botón «Check Updates» consulta las releases, descarga el
//! AppImage y deja la actualización lista; las compilaciones de desarrollo nunca
//! contactan GitHub por su cuenta.

mod common;

use common::{read_http_request_async, write_http_json_async};
use emusync_application::UpdateState;
use emusync_data_local::ConfigManager;
use emusync_desktop_app::{AppEvent, DesktopCoordinator, coordinator::UpdateSourceConfig};
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
