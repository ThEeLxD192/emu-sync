//! Signed releases end to end: the coordinator only leaves ready to install an update
//! whose asset matches the manifest the release key signed. Every other release ends in
//! a verification failure that leaves no file behind, and a newer release without a
//! manifest is never reported as "up to date" nor offered.
//!
//! `EMUSYNC_UPDATE_PUBLIC_KEY` is process-wide state read when the coordinator is built,
//! so the tests that depend on it run in a copy of this test binary with the variable
//! set or removed.

use crate::common;

use base64::{Engine, engine::general_purpose::STANDARD};
use common::{
    read_http_request_async,
    signed_release::{signed_manifest, test_release_key},
    write_http_json_async,
};
use emusync_application::{DownloadFailure, UpdateState};
use emusync_data_local::ConfigManager;
use emusync_desktop_app::{
    AppEvent, DesktopCoordinator,
    coordinator::{ProxyPolicy, UpdateSourceConfig},
};
use emusync_domain::config::Language;
use emusync_presentation::{actions::UiAction, dialogs::update_failure_message, i18n};
use ring::signature::Ed25519KeyPair;
use std::{
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc::Receiver,
    },
    time::Duration,
};
use tempfile::TempDir;
use tokio::{io::AsyncWriteExt, net::TcpListener, sync::Notify};

/// The asset this platform installs from a release that publishes both packages.
const PLATFORM_ASSET: &str = if cfg!(windows) {
    "EmuSync-x64.msi"
} else {
    "EmuSync-x86_64.AppImage"
};

/// The package of the other platform, published in the same release.
const OTHER_PLATFORM_ASSET: &str = if cfg!(windows) {
    "EmuSync-x86_64.AppImage"
} else {
    "EmuSync-x64.msi"
};

/// The bytes the test manifests sign.
const SIGNED_ASSET: &[u8] = b"signed EmuSync update bytes (verification tests)";

/// Bytes of the same length as [`SIGNED_ASSET`] that no manifest signs; only the
/// tampered-asset tests serve them, so leftovers in the temporary folder are theirs.
const TAMPERED_ASSET: &[u8] = b"forged EmuSync update bytes (verification tests)";

/// Bytes only the Spanish tampered-asset test serves.
const TAMPERED_ASSET_SPANISH: &[u8] = b"forged EmuSync update bytes (spanish dialog tst)";

// Same length as the signed bytes, so only their SHA-256 can tell them apart.
const _: () = assert!(TAMPERED_ASSET.len() == SIGNED_ASSET.len());
const _: () = assert!(TAMPERED_ASSET_SPANISH.len() == SIGNED_ASSET.len());

/// The public key of RFC 8032, section 7.1, test 1: the key `test_release_key` returns.
const TEST_PUBLIC_KEY_BASE64: &str = "11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=";

/// The public key of RFC 8032, section 7.1, test 2: a key that signs no test release.
const OTHER_PUBLIC_KEY_BASE64: &str = "PUAXw+hDiVqStwqnTRt+vJyYLM8uxJaMwM1V8Sr0Zgw=";

/// The seed of RFC 8032, section 7.1, test 2: a published key, for tests only.
const OTHER_SEED: [u8; 32] = [
    0x4c, 0xcd, 0x08, 0x9b, 0x28, 0xff, 0x96, 0xda, 0x9d, 0xb6, 0xc3, 0x46, 0xec, 0x11, 0x4e, 0x0f,
    0x5b, 0x8a, 0x31, 0x9f, 0x35, 0xab, 0xa6, 0x24, 0xda, 0x8c, 0xf6, 0xed, 0x4f, 0xb8, 0xa6, 0xfb,
];

const NEWER_TAG: &str = "v99.0.0";

/// The variable that development builds read the controlled server's key from.
const DEVELOPMENT_KEY_VARIABLE: &str = "EMUSYNC_UPDATE_PUBLIC_KEY";

/// Selects the scenario the child process runs.
const SCENARIO_VARIABLE: &str = "EMUSYNC_RELEASE_KEY_SCENARIO";

/// Which signed files the release lists.
enum SignedFiles {
    /// The manifest and its signature, with these bytes.
    Both {
        manifest: Vec<u8>,
        signature: Vec<u8>,
    },
    /// Only the manifest; the signature is missing.
    ManifestOnly { manifest: Vec<u8> },
    /// Neither: an unsigned release.
    Neither,
}

/// A controlled release server and the number of times a package was requested.
struct ReleaseServer {
    base_url: String,
    package_requests: Arc<AtomicUsize>,
}

/// Serves GitHub's latest-release API for `tag` with both packages, each answered with
/// `served_package`, and the signed files `signed_files` describes.
async fn release_server(
    tag: &'static str,
    served_package: &'static [u8],
    signed_files: SignedFiles,
) -> ReleaseServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let download = format!("{base_url}/download");
    let package_requests = Arc::new(AtomicUsize::new(0));
    let (manifest, signature) = match signed_files {
        SignedFiles::Both {
            manifest,
            signature,
        } => (Some(manifest), Some(signature)),
        SignedFiles::ManifestOnly { manifest } => (Some(manifest), None),
        SignedFiles::Neither => (None, None),
    };
    let mut assets = vec![
        format!(
            r#"{{"name":"EmuSync-x86_64.AppImage","browser_download_url":"{download}/EmuSync-x86_64.AppImage","size":{}}}"#,
            served_package.len()
        ),
        format!(
            r#"{{"name":"EmuSync-x64.msi","browser_download_url":"{download}/EmuSync-x64.msi","size":{}}}"#,
            served_package.len()
        ),
    ];
    if manifest.is_some() {
        assets.push(format!(
            r#"{{"name":"EmuSync-SHA256SUMS","browser_download_url":"{download}/EmuSync-SHA256SUMS","size":1}}"#
        ));
    }
    if signature.is_some() {
        assets.push(format!(
            r#"{{"name":"EmuSync-SHA256SUMS.sig","browser_download_url":"{download}/EmuSync-SHA256SUMS.sig","size":1}}"#
        ));
    }
    let release_body = Arc::new(format!(
        r#"{{"tag_name":"{tag}","name":"EmuSync {tag}","body":"Fixes","assets":[{}]}}"#,
        assets.join(",")
    ));
    let signed_files = Arc::new((manifest, signature));
    let requests = Arc::clone(&package_requests);
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let release_body = Arc::clone(&release_body);
            let signed_files = Arc::clone(&signed_files);
            let requests = Arc::clone(&requests);
            tokio::spawn(async move {
                let Ok((headers, _)) = read_http_request_async(&mut stream).await else {
                    return;
                };
                let binary_body: Option<&[u8]> =
                    if headers.starts_with("GET /download/EmuSync-SHA256SUMS ") {
                        signed_files.0.as_deref()
                    } else if headers.starts_with("GET /download/EmuSync-SHA256SUMS.sig ") {
                        signed_files.1.as_deref()
                    } else if headers.starts_with("GET /download/EmuSync-x86_64.AppImage ")
                        || headers.starts_with("GET /download/EmuSync-x64.msi ")
                    {
                        requests.fetch_add(1, Ordering::SeqCst);
                        Some(served_package)
                    } else {
                        None
                    };
                if let Some(body) = binary_body {
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(head.as_bytes()).await;
                    let _ = stream.write_all(body).await;
                    let _ = stream.flush().await;
                } else if headers.starts_with("GET /repos/ThEeLxD192/emu-sync/releases/latest ") {
                    let _ = write_http_json_async(&mut stream, 200, "OK", &release_body).await;
                } else {
                    let _ = write_http_json_async(&mut stream, 404, "Not Found", "{}").await;
                }
            });
        }
    });
    ReleaseServer {
        base_url,
        package_requests,
    }
}

/// A manifest of `tag` listing both packages with the bytes of [`SIGNED_ASSET`], signed
/// with the test key.
fn manifest_signed_with_the_test_key(tag: &str) -> SignedFiles {
    let (manifest, signature) = signed_manifest(
        tag,
        &[
            ("EmuSync-x86_64.AppImage", SIGNED_ASSET),
            ("EmuSync-x64.msi", SIGNED_ASSET),
        ],
    );
    SignedFiles::Both {
        manifest,
        signature,
    }
}

/// The same manifest as [`manifest_signed_with_the_test_key`], signed with another key.
fn manifest_signed_with_another_key(tag: &str) -> SignedFiles {
    let SignedFiles::Both { manifest, .. } = manifest_signed_with_the_test_key(tag) else {
        unreachable!()
    };
    let other_key_pair =
        Ed25519KeyPair::from_seed_unchecked(&OTHER_SEED).expect("the RFC 8032 seed is valid");
    let signature = format!(
        "{}\n",
        STANDARD.encode(other_key_pair.sign(&manifest).as_ref())
    );
    SignedFiles::Both {
        manifest,
        signature: signature.into_bytes(),
    }
}

/// A coordinator, the events it sends, and a notification of each one.
struct UpdateHarness {
    coordinator: DesktopCoordinator,
    event_rx: Receiver<AppEvent>,
    event_notify: Arc<Notify>,
    _config_dir: TempDir,
}

/// A coordinator built as the application builds it, without proxy, then given to
/// `configure` (which sets its release key and update source).
fn harness(configure: impl FnOnce(DesktopCoordinator) -> DesktopCoordinator) -> UpdateHarness {
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
    .with_proxy_policy(ProxyPolicy::Direct);
    UpdateHarness {
        coordinator: configure(coordinator),
        event_rx,
        event_notify,
        _config_dir: config_dir,
    }
}

/// A coordinator that trusts the test key for the controlled server at `base_url`.
fn trusting_the_test_key(base_url: &str) -> UpdateHarness {
    let source = UpdateSourceConfig::Controlled(base_url.to_owned());
    harness(|coordinator| {
        coordinator
            .with_controlled_release_key(test_release_key())
            .with_update_source(source)
    })
}

impl UpdateHarness {
    /// Waits for an event matching `predicate`, keeping every event received meanwhile
    /// in `seen` so the test can also check what was not sent.
    async fn collect_until(
        &self,
        seen: &mut Vec<AppEvent>,
        mut predicate: impl FnMut(&AppEvent) -> bool,
    ) {
        let result = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                while let Ok(event) = self.event_rx.try_recv() {
                    let is_match = predicate(&event);
                    seen.push(event);
                    if is_match {
                        return;
                    }
                }
                // Each event wakes this; a wake-up sent before waiting is kept.
                self.event_notify.notified().await;
            }
        })
        .await;
        if result.is_err() {
            panic!("timed out waiting for the event; received {seen:?}");
        }
    }

    /// Asks for a manual check, then downloads the release it offers, and returns every
    /// event until the download finishes or fails.
    async fn check_and_download(&self) -> Vec<AppEvent> {
        self.coordinator
            .handle_action(UiAction::CheckForUpdates { manual: true });
        let mut seen = Vec::new();
        self.collect_until(&mut seen, |event| {
            matches!(
                event,
                AppEvent::UpdateStateChanged(
                    UpdateState::Available(_) | UpdateState::CheckFailed(_) | UpdateState::Idle
                )
            )
        })
        .await;
        let Some(AppEvent::UpdateStateChanged(UpdateState::Available(info))) = seen.last() else {
            panic!("the signed newer release must be offered: {seen:?}");
        };
        assert_eq!(info.version, "99.0.0");
        assert_eq!(info.asset_name, PLATFORM_ASSET);

        self.coordinator
            .handle_action(UiAction::DownloadUpdate(info.clone()));
        self.collect_until(&mut seen, is_finished_download).await;
        seen
    }
}

fn is_finished_download(event: &AppEvent) -> bool {
    matches!(
        event,
        AppEvent::UpdateStateChanged(
            UpdateState::DownloadFailed(_) | UpdateState::ReadyToRestart { .. }
        )
    )
}

/// The detail of the failed transfer that ends `events`, failing the test on any other end
/// or if an update was ever ready to restart.
fn failed_transfer_detail(events: &[AppEvent]) -> &str {
    assert!(
        !events.iter().any(|event| matches!(
            event,
            AppEvent::UpdateStateChanged(UpdateState::ReadyToRestart { .. })
        )),
        "an unverified update must never be ready to restart: {events:?}"
    );
    match events.last() {
        Some(AppEvent::UpdateStateChanged(UpdateState::DownloadFailed(
            DownloadFailure::Transfer(detail),
        ))) => detail,
        other => panic!("a verification failure is a failed transfer, got {other:?}: {events:?}"),
    }
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

/// The updater stages downloads beside `APPIMAGE` and its installer replaces that file:
/// these tests need it unset to know where downloads go and to never touch a real install.
fn assert_not_running_from_an_app_image() {
    assert!(
        std::env::var_os("APPIMAGE").is_none(),
        "run this test without APPIMAGE: the updater would use the running AppImage"
    );
}

/// Update downloads (finished or partial) left in the temporary folder with `content`.
fn leftover_downloads_with(content: &[u8]) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(std::env::temp_dir())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains("EmuSync-update"))
        })
        .filter(|path| std::fs::read(path).is_ok_and(|bytes| bytes == content))
        .collect()
}

/// Waits until no download with `content` remains: the failure may be published just
/// before its file is removed.
async fn assert_no_download_left_with(content: &[u8]) {
    let removed = tokio::time::timeout(Duration::from_secs(10), async {
        while !leftover_downloads_with(content).is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(
        removed.is_ok(),
        "an unverified update stays on disk: {:?}",
        leftover_downloads_with(content)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_package_whose_bytes_differ_from_the_signed_manifest_fails_verification_and_leaves_no_file()
 {
    assert_not_running_from_an_app_image();
    let server = release_server(
        NEWER_TAG,
        TAMPERED_ASSET,
        manifest_signed_with_the_test_key(NEWER_TAG),
    )
    .await;
    let updates = trusting_the_test_key(&server.base_url);

    let events = updates.check_and_download().await;

    assert_eq!(
        failed_transfer_detail(&events),
        "Update verification failed: the downloaded update does not match its signed SHA-256"
    );
    assert_no_download_left_with(TAMPERED_ASSET).await;
    assert_eq!(
        toasts(&events),
        Vec::<&str>::new(),
        "the dialog explains it"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_manifest_signed_with_another_key_fails_verification_without_downloading_the_package() {
    let server = release_server(
        NEWER_TAG,
        SIGNED_ASSET,
        manifest_signed_with_another_key(NEWER_TAG),
    )
    .await;
    let updates = trusting_the_test_key(&server.base_url);

    let events = updates.check_and_download().await;

    assert_eq!(
        failed_transfer_detail(&events),
        "Update verification failed: the release manifest is not signed with the trusted release key"
    );
    assert_eq!(
        server.package_requests.load(Ordering::SeqCst),
        0,
        "a package the key did not sign is never fetched"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_manifest_signed_for_another_release_fails_verification_without_downloading_the_package()
{
    // A genuine, older signed manifest replayed in a newer release.
    let server = release_server(
        NEWER_TAG,
        SIGNED_ASSET,
        manifest_signed_with_the_test_key("v98.0.0"),
    )
    .await;
    let updates = trusting_the_test_key(&server.base_url);

    let events = updates.check_and_download().await;

    assert_eq!(
        failed_transfer_detail(&events),
        "Update verification failed: the release manifest belongs to another release"
    );
    assert_eq!(server.package_requests.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_manifest_that_only_signs_the_other_platform_package_fails_verification() {
    let (manifest, signature) = signed_manifest(NEWER_TAG, &[(OTHER_PLATFORM_ASSET, SIGNED_ASSET)]);
    let server = release_server(
        NEWER_TAG,
        SIGNED_ASSET,
        SignedFiles::Both {
            manifest,
            signature,
        },
    )
    .await;
    let updates = trusting_the_test_key(&server.base_url);

    let events = updates.check_and_download().await;

    assert_eq!(
        failed_transfer_detail(&events),
        format!("Update verification failed: the release manifest does not list {PLATFORM_ASSET}")
    );
    assert_eq!(server.package_requests.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_manual_check_of_a_newer_unsigned_release_says_it_could_not_check_never_up_to_date_or_available()
 {
    let without_signature = signed_manifest(NEWER_TAG, &[(PLATFORM_ASSET, SIGNED_ASSET)]).0;
    for (case, signed_files) in [
        ("no manifest", SignedFiles::Neither),
        (
            "manifest without signature",
            SignedFiles::ManifestOnly {
                manifest: without_signature,
            },
        ),
    ] {
        let server = release_server(NEWER_TAG, SIGNED_ASSET, signed_files).await;
        let updates = trusting_the_test_key(&server.base_url);

        updates
            .coordinator
            .handle_action(UiAction::CheckForUpdates { manual: true });

        let mut seen = Vec::new();
        updates
            .collect_until(&mut seen, |event| matches!(event, AppEvent::Toast(_)))
            .await;
        assert!(
            seen.iter().any(|event| matches!(
                event,
                AppEvent::UpdateStateChanged(UpdateState::CheckFailed(detail))
                    if detail == "Update verification failed: release v99.0.0 is not signed"
            )),
            "{case}: {seen:?}"
        );
        assert_eq!(
            toasts(&seen),
            vec![
                "Could not check for updates: Update verification failed: release v99.0.0 is not signed"
            ],
            "{case}: {seen:?}"
        );
        assert!(
            !seen.iter().any(|event| matches!(
                event,
                AppEvent::UpdateStateChanged(UpdateState::Idle | UpdateState::Available(_))
            )),
            "{case}: an unsigned release is neither \"up to date\" nor an update: {seen:?}"
        );
        assert_eq!(server.package_requests.load(Ordering::SeqCst), 0, "{case}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_manual_check_of_the_running_version_published_unsigned_still_says_up_to_date() {
    // Releases published before signing existed carry no manifest; when they are not
    // newer, there is nothing to verify.
    let server = release_server(
        concat!("v", env!("CARGO_PKG_VERSION")),
        SIGNED_ASSET,
        SignedFiles::Neither,
    )
    .await;
    let updates = trusting_the_test_key(&server.base_url);

    updates
        .coordinator
        .handle_action(UiAction::CheckForUpdates { manual: true });

    let mut seen = Vec::new();
    updates
        .collect_until(&mut seen, |event| matches!(event, AppEvent::Toast(_)))
        .await;
    assert_eq!(toasts(&seen), vec!["EmuSync is up to date."], "{seen:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_manual_check_in_spanish_of_a_newer_unsigned_release_reports_it_in_spanish() {
    let server = release_server(NEWER_TAG, SIGNED_ASSET, SignedFiles::Neither).await;
    let updates = trusting_the_test_key(&server.base_url);
    updates.coordinator.language.set(Language::Spanish);

    updates
        .coordinator
        .handle_action(UiAction::CheckForUpdates { manual: true });

    let mut seen = Vec::new();
    updates
        .collect_until(&mut seen, |event| matches!(event, AppEvent::Toast(_)))
        .await;
    // The detail from the updater is shown as is inside the translated text.
    assert_eq!(
        toasts(&seen),
        vec![
            "No se pudo comprobar si hay actualizaciones: Update verification failed: release v99.0.0 is not signed"
        ],
        "{seen:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tampered_package_explains_the_failed_verification_in_spanish_in_the_update_dialog() {
    assert_not_running_from_an_app_image();
    let server = release_server(
        NEWER_TAG,
        TAMPERED_ASSET_SPANISH,
        manifest_signed_with_the_test_key(NEWER_TAG),
    )
    .await;
    let updates = trusting_the_test_key(&server.base_url);
    updates.coordinator.language.set(Language::Spanish);

    let events = updates.check_and_download().await;

    let Some(AppEvent::UpdateStateChanged(state)) = events.last() else {
        panic!("the download must end in a state: {events:?}");
    };
    assert_eq!(
        update_failure_message(i18n::texts(Language::Spanish), state).as_deref(),
        Some(
            "No se pudo descargar la actualización: Update verification failed: the downloaded update does not match its signed SHA-256"
        ),
        "{events:?}"
    );
    assert_no_download_left_with(TAMPERED_ASSET_SPANISH).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_controlled_release_key_survives_the_rebuilds_of_later_settings() {
    assert_not_running_from_an_app_image();
    let server = release_server(
        NEWER_TAG,
        SIGNED_ASSET,
        manifest_signed_with_the_test_key(NEWER_TAG),
    )
    .await;
    let source = UpdateSourceConfig::Controlled(server.base_url.clone());
    // Source first, then the key, then a setting that rebuilds the updater again.
    let updates = harness(|coordinator| {
        coordinator
            .with_update_source(source)
            .with_controlled_release_key(test_release_key())
            .with_proxy_policy(ProxyPolicy::Direct)
    });

    let events = updates.check_and_download().await;

    let Some(AppEvent::UpdateStateChanged(UpdateState::ReadyToRestart {
        downloaded_path, ..
    })) = events.last()
    else {
        panic!("the signed package must be ready to restart: {events:?}");
    };
    let downloaded = std::fs::read(downloaded_path).unwrap();
    std::fs::remove_file(downloaded_path).unwrap();
    assert_eq!(downloaded, SIGNED_ASSET);
}

// Scenarios that depend on `EMUSYNC_UPDATE_PUBLIC_KEY`, run in a child process.

/// The coordinator keeps its own key and rejects a manifest signed with the test key.
const EMBEDDED_KEY_WITHOUT_VARIABLE: &str = "embedded-key-without-variable";
/// The variable holds no valid key: the coordinator falls back to its own key.
const EMBEDDED_KEY_WITH_INVALID_VARIABLE: &str = "embedded-key-with-invalid-variable";
/// The variable holds the test key: only a development build trusts it.
const TEST_KEY_IN_VARIABLE: &str = "test-key-in-variable";
/// The variable holds another key, but the coordinator was given the test key.
const EXPLICIT_KEY_OVER_VARIABLE: &str = "explicit-key-over-variable";

/// Runs `scenario` in a child process whose `EMUSYNC_UPDATE_PUBLIC_KEY` is `variable`
/// (removed when `None`).
fn run_release_key_scenario(scenario: &str, variable: Option<&str>) {
    assert_not_running_from_an_app_image();
    // The child runs `release_key_child` by its full name in the test binary
    // (`update_verification_tests::...`).
    let module = module_path!().split_once("::").unwrap().1;
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            &format!("{module}::release_key_child"),
            "--ignored",
            "--nocapture",
        ])
        .env(SCENARIO_VARIABLE, scenario)
        .env_remove("EMUSYNC_UPDATE_BASE_URL");
    match variable {
        Some(value) => command.env(DEVELOPMENT_KEY_VARIABLE, value),
        None => command.env_remove(DEVELOPMENT_KEY_VARIABLE),
    };
    let output = command.output().unwrap();

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
}

#[test]
fn a_coordinator_without_a_controlled_key_rejects_a_manifest_signed_with_the_test_key() {
    run_release_key_scenario(EMBEDDED_KEY_WITHOUT_VARIABLE, None);
}

#[test]
fn an_invalid_development_key_variable_keeps_the_embedded_key_instead_of_disabling_verification() {
    run_release_key_scenario(EMBEDDED_KEY_WITH_INVALID_VARIABLE, Some("not a key"));
}

#[test]
fn the_development_key_variable_is_trusted_only_in_development_builds() {
    run_release_key_scenario(TEST_KEY_IN_VARIABLE, Some(TEST_PUBLIC_KEY_BASE64));
}

#[test]
fn a_key_given_to_the_coordinator_wins_over_the_development_key_variable() {
    run_release_key_scenario(EXPLICIT_KEY_OVER_VARIABLE, Some(OTHER_PUBLIC_KEY_BASE64));
}

/// Runs the scenario named in `EMUSYNC_RELEASE_KEY_SCENARIO`, in a process whose
/// `EMUSYNC_UPDATE_PUBLIC_KEY` the parent test set.
#[test]
#[ignore = "re-executed as a child process with its own EMUSYNC_UPDATE_PUBLIC_KEY"]
fn release_key_child() {
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
            let server = release_server(
                NEWER_TAG,
                SIGNED_ASSET,
                manifest_signed_with_the_test_key(NEWER_TAG),
            )
            .await;
            let source = UpdateSourceConfig::Controlled(server.base_url.clone());
            let trusts_the_test_key = match scenario.as_str() {
                EMBEDDED_KEY_WITHOUT_VARIABLE | EMBEDDED_KEY_WITH_INVALID_VARIABLE => false,
                TEST_KEY_IN_VARIABLE => cfg!(debug_assertions),
                EXPLICIT_KEY_OVER_VARIABLE => true,
                other => panic!("unknown scenario {other}"),
            };
            let updates = harness(|coordinator| {
                if scenario == EXPLICIT_KEY_OVER_VARIABLE {
                    coordinator
                        .with_controlled_release_key(test_release_key())
                        .with_update_source(source)
                } else {
                    coordinator.with_update_source(source)
                }
            });

            let events = updates.check_and_download().await;

            if trusts_the_test_key {
                let Some(AppEvent::UpdateStateChanged(UpdateState::ReadyToRestart {
                    downloaded_path,
                    ..
                })) = events.last()
                else {
                    panic!("{scenario}: the test key must be trusted: {events:?}");
                };
                let downloaded = std::fs::read(downloaded_path).unwrap();
                std::fs::remove_file(downloaded_path).unwrap();
                assert_eq!(downloaded, SIGNED_ASSET, "{scenario}");
            } else {
                assert_eq!(
                    failed_transfer_detail(&events),
                    "Update verification failed: the release manifest is not signed with the trusted release key",
                    "{scenario}"
                );
                assert_eq!(server.package_requests.load(Ordering::SeqCst), 0, "{scenario}");
            }
        });
}
