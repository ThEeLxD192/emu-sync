//! Download progress as it reaches the interface, through the real downloader, update
//! coordinator and staging files: no update the application rejects (signed hash, size
//! announced by the release, truncated transfer) is ever shown at 100 %, and a valid one
//! only reaches 100 % as `ReadyToRestart`.

use crate::common;

use common::signed_release::{signed_manifest, test_release_key};
use common::{read_http_request_async, signed_release::SignedFilesServer};
use emusync_application::{DownloadFailure, UpdateState};
use emusync_data_local::ConfigManager;
use emusync_desktop_app::{
    AppEvent, DesktopCoordinator,
    coordinator::{ProxyPolicy, UpdateSourceConfig},
};
use emusync_domain::update::UpdateInfo;
use emusync_presentation::actions::UiAction;
use std::{
    sync::{Arc, mpsc::Receiver},
    time::Duration,
};
use tempfile::TempDir;
use tokio::{io::AsyncWriteExt, net::TcpListener, sync::Notify};

/// The asset this platform installs.
const PLATFORM_ASSET: &str = if cfg!(windows) {
    "EmuSync-x64.msi"
} else {
    "EmuSync-x86_64.AppImage"
};

/// The lowest progress the interface shows as «100%»: it shows whole percentages,
/// rounded.
const LOWEST_PROGRESS_SHOWN_AS_ONE_HUNDRED_PERCENT: f32 = 0.995;

/// Size of the multi-chunk packages, so progress is reported many times on the way.
const LARGE_PACKAGE_BYTES: usize = 256 * 1024;

/// A package of `LARGE_PACKAGE_BYTES` filled with `fill`; each test uses its own `fill`
/// so its leftovers in the temporary folder are told apart from other tests'.
fn large_package(fill: u8) -> Vec<u8> {
    vec![fill; LARGE_PACKAGE_BYTES]
}

/// Answers every request with `response` and closes the connection.
async fn package_server(response: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let response = Arc::new(response);
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let response = Arc::clone(&response);
            tokio::spawn(async move {
                if read_http_request_async(&mut stream).await.is_err() {
                    return;
                }
                let _ = stream.write_all(&response).await;
                let _ = stream.flush().await;
                let _ = stream.shutdown().await;
            });
        }
    });
    base_url
}

/// A complete response with `body`, announcing `announced_bytes` as its length.
fn package_response(body: &[u8], announced_bytes: usize) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {announced_bytes}\r\nConnection: close\r\n\r\n"
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}

/// A coordinator as the application builds it, its events and a notification of each.
struct UpdateHarness {
    coordinator: DesktopCoordinator,
    event_rx: Receiver<AppEvent>,
    event_notify: Arc<Notify>,
    _config_dir: TempDir,
}

fn harness(package_base_url: &str) -> UpdateHarness {
    let config_dir = tempfile::tempdir().unwrap();
    let manager = ConfigManager::new(&config_dir.path().join("config.json")).unwrap();
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    let event_notify = Arc::new(Notify::new());
    let notify = Arc::clone(&event_notify);
    let coordinator = DesktopCoordinator::new(
        manager,
        event_tx,
        Arc::new(move || notify.notify_one()),
        tokio::runtime::Handle::current(),
    )
    .with_proxy_policy(ProxyPolicy::Direct)
    .with_controlled_release_key(test_release_key())
    .with_update_source(UpdateSourceConfig::Controlled(package_base_url.to_owned()));
    UpdateHarness {
        coordinator,
        event_rx,
        event_notify,
        _config_dir: config_dir,
    }
}

impl UpdateHarness {
    /// Downloads the package that `package_base_url` serves for release v99.0.0, whose
    /// manifest signs `signed_bytes` and which announces `asset_size` bytes, and returns
    /// every update state published until the download ends.
    async fn download(
        &self,
        package_base_url: &str,
        signed_bytes: &[u8],
        asset_size: usize,
    ) -> Vec<UpdateState> {
        let (manifest, signature) = signed_manifest("v99.0.0", &[(PLATFORM_ASSET, signed_bytes)]);
        let signed_files = SignedFilesServer::serve(manifest, signature).await;
        let info = UpdateInfo {
            version: "99.0.0".into(),
            tag_name: "v99.0.0".into(),
            release_notes: "Fixes".into(),
            download_url: format!("{package_base_url}/download/{PLATFORM_ASSET}"),
            asset_size: asset_size as i64,
            asset_name: PLATFORM_ASSET.into(),
            manifest_url: signed_files.manifest_url.clone(),
            signature_url: signed_files.signature_url.clone(),
        };

        self.coordinator
            .handle_action(UiAction::DownloadUpdate(info));

        let mut states = Vec::new();
        let finished = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                while let Ok(event) = self.event_rx.try_recv() {
                    if let AppEvent::UpdateStateChanged(state) = event {
                        let is_finished = matches!(
                            state,
                            UpdateState::DownloadFailed(_) | UpdateState::ReadyToRestart { .. }
                        );
                        states.push(state);
                        if is_finished {
                            return;
                        }
                    }
                }
                // Each event wakes this; a wake-up sent before waiting is kept.
                self.event_notify.notified().await;
            }
        })
        .await;
        assert!(
            finished.is_ok(),
            "the download did not finish; received {states:?}"
        );
        states
    }
}

/// The `Downloading` progress values among `states` that would read «100%».
fn shown_as_complete(states: &[UpdateState]) -> Vec<f32> {
    states
        .iter()
        .filter_map(|state| match state {
            UpdateState::Downloading { progress, .. }
                if *progress >= LOWEST_PROGRESS_SHOWN_AS_ONE_HUNDRED_PERCENT =>
            {
                Some(*progress)
            }
            _ => None,
        })
        .collect()
}

fn downloading_progress(states: &[UpdateState]) -> Vec<f32> {
    states
        .iter()
        .filter_map(|state| match state {
            UpdateState::Downloading { progress, .. } => Some(*progress),
            _ => None,
        })
        .collect()
}

/// The updater stages downloads beside `APPIMAGE`: these tests need it unset to never
/// touch a real install.
fn assert_not_running_from_an_app_image() {
    assert!(
        std::env::var_os("APPIMAGE").is_none(),
        "run this test without APPIMAGE: the updater would use the running AppImage"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_signed_package_whose_size_differs_from_the_release_is_never_shown_at_one_hundred_percent()
 {
    assert_not_running_from_an_app_image();
    // The manifest signs the bytes served: only the size the release announces is wrong,
    // and that is checked after the downloader has put the file in place.
    let package = large_package(b'S');
    let package_base_url = package_server(package_response(&package, package.len())).await;
    let updates = harness(&package_base_url);

    let states = updates
        .download(&package_base_url, &package, package.len() + 100)
        .await;

    assert_eq!(
        states.last(),
        Some(&UpdateState::DownloadFailed(
            DownloadFailure::SizeMismatch {
                expected_bytes: LARGE_PACKAGE_BYTES as u64 + 100,
                received_bytes: LARGE_PACKAGE_BYTES as u64,
            }
        )),
        "{states:?}"
    );
    assert!(
        !downloading_progress(&states).is_empty(),
        "the bytes were received: {states:?}"
    );
    assert_eq!(
        shown_as_complete(&states),
        Vec::<f32>::new(),
        "a rejected update was shown at 100 %"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_package_that_fails_its_signed_hash_is_never_shown_at_one_hundred_percent() {
    assert_not_running_from_an_app_image();
    let tampered_package = large_package(b'T');
    let signed_package = large_package(b'U');
    let package_base_url =
        package_server(package_response(&tampered_package, tampered_package.len())).await;
    let updates = harness(&package_base_url);

    let states = updates
        .download(&package_base_url, &signed_package, tampered_package.len())
        .await;

    assert_eq!(
        states.last(),
        Some(&UpdateState::DownloadFailed(DownloadFailure::Transfer(
            "Update verification failed: the downloaded update does not match its signed SHA-256"
                .into()
        ))),
        "{states:?}"
    );
    assert!(
        !downloading_progress(&states).is_empty(),
        "the bytes were received: {states:?}"
    );
    assert_eq!(
        shown_as_complete(&states),
        Vec::<f32>::new(),
        "a rejected update was shown at 100 %"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_package_cut_one_byte_short_of_its_announced_length_is_never_shown_at_one_hundred_percent()
 {
    assert_not_running_from_an_app_image();
    // One byte short of the announced length: 99.9996 % of the bytes arrive.
    let package = large_package(b'C');
    let package_base_url = package_server(package_response(
        &package[..package.len() - 1],
        package.len(),
    ))
    .await;
    let updates = harness(&package_base_url);

    let states = updates
        .download(&package_base_url, &package, package.len())
        .await;

    assert!(
        matches!(
            states.last(),
            Some(UpdateState::DownloadFailed(DownloadFailure::Transfer(_)))
        ),
        "a cut transfer is a failed transfer: {states:?}"
    );
    assert!(
        !downloading_progress(&states).is_empty(),
        "the bytes were received: {states:?}"
    );
    assert_eq!(
        shown_as_complete(&states),
        Vec::<f32>::new(),
        "a rejected update was shown at 100 %"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_valid_package_reaches_one_hundred_percent_only_as_ready_to_restart() {
    assert_not_running_from_an_app_image();
    let package = large_package(b'V');
    let package_base_url = package_server(package_response(&package, package.len())).await;
    let updates = harness(&package_base_url);

    let states = updates
        .download(&package_base_url, &package, package.len())
        .await;

    let Some(UpdateState::ReadyToRestart {
        downloaded_path, ..
    }) = states.last()
    else {
        panic!("a valid package must be ready to restart: {states:?}");
    };
    let downloaded = std::fs::read(downloaded_path).unwrap();
    std::fs::remove_file(downloaded_path).unwrap();
    assert!(downloaded == package, "the package must arrive whole");
    assert_eq!(
        shown_as_complete(&states),
        Vec::<f32>::new(),
        "only ReadyToRestart means complete"
    );
    let progress = downloading_progress(&states);
    assert!(
        progress.len() > 1 && progress.windows(2).all(|pair| pair[0] <= pair[1]),
        "progress advances and never goes back: {progress:?}"
    );
}
