use emusync_application::{DownloadFailure, UpdateCoordinator, UpdateState};
use emusync_domain::update::{UpdateFiles, UpdateInfo, UpdateInstaller, UpdateSource};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct MockSource {
    next_info: Arc<Mutex<Option<UpdateInfo>>>,
    check_error: Arc<Mutex<Option<String>>>,
    download_error: Arc<Mutex<Option<String>>>,
    download_started: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    stall_download: Arc<Mutex<bool>>,
    check_started: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    check_resume: Arc<Mutex<Option<tokio::sync::oneshot::Receiver<()>>>>,
}

impl UpdateSource for MockSource {
    type Error = String;

    async fn check(&self) -> Result<Option<UpdateInfo>, Self::Error> {
        let start_tx = self.check_started.lock().unwrap().take();
        if let Some(tx) = start_tx {
            let _ = tx.send(());
        }
        let resume_rx = self.check_resume.lock().unwrap().take();
        if let Some(rx) = resume_rx {
            let _ = rx.await;
        }
        if let Some(err) = self.check_error.lock().unwrap().clone() {
            return Err(err);
        }
        Ok(self.next_info.lock().unwrap().clone())
    }

    async fn download(
        &self,
        _info: &UpdateInfo,
        _destination: &str,
        mut on_progress: impl FnMut(f32) + Send,
    ) -> Result<(), Self::Error> {
        on_progress(0.5);
        let start_tx = self.download_started.lock().unwrap().take();
        if let Some(tx) = start_tx {
            let _ = tx.send(());
        }
        let stall = *self.stall_download.lock().unwrap();
        if stall {
            std::future::pending::<()>().await;
        }
        if let Some(err) = self.download_error.lock().unwrap().clone() {
            return Err(err);
        }
        on_progress(1.0);
        Ok(())
    }
}

#[derive(Clone, Default)]
struct MockFiles {
    deleted_paths: Arc<Mutex<Vec<String>>>,
    destination: Arc<Mutex<String>>,
    custom_size: Arc<Mutex<Option<u64>>>,
    prepare_error: Arc<Mutex<Option<String>>>,
    size_error: Arc<Mutex<Option<String>>>,
    stall_prepare: Arc<Mutex<bool>>,
}

impl UpdateFiles for MockFiles {
    type Error = String;

    async fn prepare_destination(&self, _info: &UpdateInfo) -> Result<String, Self::Error> {
        let stall = *self.stall_prepare.lock().unwrap();
        if stall {
            std::future::pending::<()>().await;
        }
        if let Some(error) = self.prepare_error.lock().unwrap().clone() {
            return Err(error);
        }
        Ok(self.destination.lock().unwrap().clone())
    }

    fn size(&self, _path: &str) -> Result<u64, Self::Error> {
        if let Some(error) = self.size_error.lock().unwrap().clone() {
            return Err(error);
        }
        if let Some(sz) = *self.custom_size.lock().unwrap() {
            return Ok(sz);
        }
        Ok(1024)
    }

    fn delete(&self, path: &str) -> Result<(), Self::Error> {
        self.deleted_paths.lock().unwrap().push(path.to_string());
        Ok(())
    }
}

#[derive(Clone, Default)]
struct MockInstaller {
    restarted_path: Arc<Mutex<Option<String>>>,
    restart_result: Arc<Mutex<Option<Result<bool, String>>>>,
}

impl UpdateInstaller for MockInstaller {
    type Error = String;

    fn is_steam_game_mode(&self) -> Result<bool, Self::Error> {
        Ok(false)
    }

    fn restart(&self, path: &str) -> Result<bool, Self::Error> {
        if let Some(res) = self.restart_result.lock().unwrap().clone() {
            return res;
        }
        *self.restarted_path.lock().unwrap() = Some(path.to_string());
        Ok(true)
    }
}

fn sample_update_info() -> UpdateInfo {
    UpdateInfo {
        version: "2.0.0".into(),
        tag_name: "v2.0.0".into(),
        release_notes: "New release".into(),
        download_url: "https://example.com/app.tar.gz".into(),
        asset_size: 1024,
        asset_name: "app.tar.gz".into(),
        manifest_url: String::new(),
        signature_url: String::new(),
    }
}

/// Awaits `step`, failing instead of hanging when a regression keeps it from finishing.
async fn within_harness_limit<T>(what: &str, step: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(10), step)
        .await
        .unwrap_or_else(|_| panic!("{what} did not finish within the harness limit"))
}

/// Waits for the source to report that a check reached it.
async fn check_started_within_limit(check_started: tokio::sync::oneshot::Receiver<()>) {
    within_harness_limit("the check reaching the source", check_started)
        .await
        .expect("the source reports the check as started");
}

#[tokio::test]
async fn update_check_publishes_state_transitions() {
    let source = MockSource::default();
    let files = MockFiles::default();
    let installer = MockInstaller::default();

    let published = Arc::new(Mutex::new(Vec::new()));
    let published_clone = Arc::clone(&published);

    let coordinator = UpdateCoordinator::new(
        source.clone(),
        files.clone(),
        installer.clone(),
        move |state| {
            published_clone.lock().unwrap().push(state.clone());
        },
    );

    // 1. Check with no update -> Idle
    coordinator.check_for_updates().await;
    assert_eq!(
        *published.lock().unwrap(),
        vec![UpdateState::Checking, UpdateState::Idle]
    );

    // 2. Check with update -> Available
    let info = sample_update_info();
    *source.next_info.lock().unwrap() = Some(info.clone());
    published.lock().unwrap().clear();

    coordinator.check_for_updates().await;
    assert_eq!(
        *published.lock().unwrap(),
        vec![UpdateState::Checking, UpdateState::Available(info)]
    );
}

#[tokio::test]
async fn failed_download_cleans_up_destination_file() {
    let source = MockSource::default();
    let files = MockFiles::default();
    let installer = MockInstaller::default();

    *files.destination.lock().unwrap() = "/tmp/downloaded_update.bin".into();
    *source.download_error.lock().unwrap() = Some("Network cut".into());

    let published = Arc::new(Mutex::new(Vec::new()));
    let published_clone = Arc::clone(&published);

    let coordinator = UpdateCoordinator::new(
        source.clone(),
        files.clone(),
        installer.clone(),
        move |state| {
            published_clone.lock().unwrap().push(state.clone());
        },
    );

    let info = sample_update_info();
    coordinator.download(&info).await;

    // Must delete destination upon download failure
    let deleted = files.deleted_paths.lock().unwrap().clone();
    assert_eq!(deleted, vec!["/tmp/downloaded_update.bin"]);

    // The source's detail is published as a transfer failure, without added wording.
    assert_eq!(
        published.lock().unwrap().last(),
        Some(&UpdateState::DownloadFailed(DownloadFailure::Transfer(
            "Network cut".into()
        )))
    );
}

#[tokio::test]
async fn successful_download_publishes_progress_and_ready_to_restart() {
    let source = MockSource::default();
    let files = MockFiles::default();
    let installer = MockInstaller::default();

    *files.destination.lock().unwrap() = "/tmp/update.tar.gz".into();

    let published = Arc::new(Mutex::new(Vec::new()));
    let published_clone = Arc::clone(&published);

    let coordinator = UpdateCoordinator::new(
        source.clone(),
        files.clone(),
        installer.clone(),
        move |state| {
            published_clone.lock().unwrap().push(state.clone());
        },
    );

    let info = sample_update_info();
    coordinator.download(&info).await;

    // File must NOT be deleted upon success
    let deleted = files.deleted_paths.lock().unwrap().clone();
    assert!(deleted.is_empty());

    // States must include initial downloading (0.0), intermediate progress, and ReadyToRestart
    let states = published.lock().unwrap().clone();
    assert!(matches!(states[0], UpdateState::Downloading { progress, .. } if progress == 0.0));
    assert!(matches!(states[1], UpdateState::Downloading { progress, .. } if progress == 0.5));
    assert_eq!(
        *states.last().unwrap(),
        UpdateState::ReadyToRestart {
            downloaded_path: "/tmp/update.tar.gz".into(),
            is_steam_game_mode: false,
        }
    );
}

#[test]
fn restart_is_blocked_when_operation_is_in_progress() {
    let source = MockSource::default();
    let files = MockFiles::default();
    let installer = MockInstaller::default();

    let coordinator = UpdateCoordinator::new(source, files, installer.clone(), |_| {});

    // Blocked if operation in progress
    let res_blocked = coordinator.restart("/path/to/update", true);
    assert!(!res_blocked);
    assert_eq!(*installer.restarted_path.lock().unwrap(), None);

    // Allowed when idle
    let res_allowed = coordinator.restart("/path/to/update", false);
    assert!(res_allowed);
    assert_eq!(
        *installer.restarted_path.lock().unwrap(),
        Some("/path/to/update".to_string())
    );
}

#[tokio::test]
async fn cancelled_download_cleans_up_destination_and_restores_available_state() {
    let source = MockSource::default();
    *source.stall_download.lock().unwrap() = true;
    let (tx, rx) = tokio::sync::oneshot::channel();
    *source.download_started.lock().unwrap() = Some(tx);

    let files = MockFiles::default();
    *files.destination.lock().unwrap() = "/tmp/update-cancel.tar.gz".to_string();
    let installer = MockInstaller::default();

    let published = Arc::new(Mutex::new(Vec::new()));
    let published_clone = Arc::clone(&published);

    let coordinator = Arc::new(UpdateCoordinator::new(
        source,
        files.clone(),
        installer,
        move |state| {
            published_clone.lock().unwrap().push(state.clone());
        },
    ));

    let info = sample_update_info();
    let coord_task = Arc::clone(&coordinator);
    let info_task = info.clone();
    let handle = tokio::spawn(async move {
        coord_task.download(&info_task).await;
    });

    // Wait until download starts and stalls
    within_harness_limit("the download reaching the source", rx)
        .await
        .expect("download should have started");

    // While downloading, can_check should be false
    assert!(!coordinator.can_check());

    // Abort the task to trigger cancellation drop
    handle.abort();
    let _ = handle.await;

    // Verify temp file was cleaned up on cancellation
    let deleted = files.deleted_paths.lock().unwrap().clone();
    assert_eq!(deleted, vec!["/tmp/update-cancel.tar.gz"]);

    // Verify coordinator is no longer busy and state reverted to Available
    assert!(coordinator.can_check());
    assert_eq!(coordinator.state(), UpdateState::Available(info));
}

#[tokio::test]
async fn reentrant_on_state_does_not_deadlock_and_can_check_while_checking() {
    type TestCoordinator = UpdateCoordinator<MockSource, MockFiles, MockInstaller>;

    let source = MockSource::default();
    let files = MockFiles::default();
    let installer = MockInstaller::default();

    let coord_holder: Arc<Mutex<Option<Arc<TestCoordinator>>>> = Arc::new(Mutex::new(None));
    let coord_holder_clone = Arc::clone(&coord_holder);

    let observed_states_in_callback = Arc::new(Mutex::new(Vec::new()));
    let observed_clone = Arc::clone(&observed_states_in_callback);

    let coordinator = Arc::new(UpdateCoordinator::new(
        source,
        files,
        installer,
        move |_state| {
            if let Some(coord) = coord_holder_clone.lock().unwrap().as_ref() {
                // Must not deadlock when calling state() or can_check() inside on_state
                let current = coord.state();
                let can = coord.can_check();
                observed_clone.lock().unwrap().push((current, can));
            }
        },
    ));

    *coord_holder.lock().unwrap() = Some(Arc::clone(&coordinator));

    // can_check() is true initially
    assert!(coordinator.can_check());

    // check_for_updates should not deadlock
    coordinator.check_for_updates().await;

    // Both state() and can_check() were queryable inside on_state without deadlock
    let observed = observed_states_in_callback.lock().unwrap().clone();
    assert!(!observed.is_empty());
}

#[tokio::test]
async fn download_size_mismatch_deletes_file_and_publishes_error_state() {
    let source = MockSource::default();
    let files = MockFiles::default();
    *files.destination.lock().unwrap() = "/tmp/size_mismatch.tar.gz".into();
    *files.custom_size.lock().unwrap() = Some(2048);

    let installer = MockInstaller::default();

    let published = Arc::new(Mutex::new(Vec::new()));
    let published_clone = Arc::clone(&published);

    let coordinator = UpdateCoordinator::new(source, files.clone(), installer, move |state| {
        published_clone.lock().unwrap().push(state.clone());
    });

    let mut info = sample_update_info();
    info.asset_size = 4096;

    coordinator.download(&info).await;

    // Verify DownloadGuard deleted the file
    assert_eq!(
        *files.deleted_paths.lock().unwrap(),
        vec!["/tmp/size_mismatch.tar.gz".to_string()]
    );

    assert_eq!(
        coordinator.state(),
        UpdateState::DownloadFailed(DownloadFailure::SizeMismatch {
            expected_bytes: 4096,
            received_bytes: 2048,
        })
    );

    // Never published ReadyToRestart
    let all_published = published.lock().unwrap().clone();
    assert!(
        !all_published
            .iter()
            .any(|s| matches!(s, UpdateState::ReadyToRestart { .. })),
        "ReadyToRestart must never be published on size mismatch"
    );

    // Resets can_check to true
    assert!(coordinator.can_check());
}

#[tokio::test]
async fn stale_or_cancelled_check_does_not_overwrite_concurrent_download_or_ready_to_restart_and_failed_restart_publishes_error()
 {
    let source = MockSource::default();
    let files = MockFiles::default();
    *files.destination.lock().unwrap() = "/tmp/update-ready.tar.gz".into();
    let installer = MockInstaller::default();

    let (check_start_tx, check_start_rx) = tokio::sync::oneshot::channel();
    let (check_resume_tx, check_resume_rx) = tokio::sync::oneshot::channel();
    *source.check_started.lock().unwrap() = Some(check_start_tx);
    *source.check_resume.lock().unwrap() = Some(check_resume_rx);

    let info = sample_update_info();
    *source.next_info.lock().unwrap() = Some(info.clone());

    let published = Arc::new(Mutex::new(Vec::new()));
    let published_clone = Arc::clone(&published);

    let coordinator = Arc::new(UpdateCoordinator::new(
        source,
        files,
        installer.clone(),
        move |state| {
            published_clone.lock().unwrap().push(state.clone());
        },
    ));

    // Launch slow update check
    let coord_task = Arc::clone(&coordinator);
    let check_task = tokio::spawn(async move {
        coord_task.check_for_updates().await;
    });

    // Wait until check_for_updates has started and suspended
    check_started_within_limit(check_start_rx).await;

    // Perform download while check is pending, transitioning state to ReadyToRestart
    within_harness_limit(
        "a download during a pending check",
        coordinator.download(&info),
    )
    .await;
    assert!(matches!(
        coordinator.state(),
        UpdateState::ReadyToRestart { .. }
    ));

    // Resume the stale check task
    check_resume_tx.send(()).expect("check_resume send failed");
    within_harness_limit("the stale check", check_task)
        .await
        .expect("check_task join failed");

    // Check must NOT overwrite ReadyToRestart with Available
    assert!(
        matches!(coordinator.state(), UpdateState::ReadyToRestart { .. }),
        "Stale check must not overwrite ReadyToRestart"
    );

    // Test failed restart publishes error state
    *installer.restart_result.lock().unwrap() = Some(Ok(false));
    let restart_success = coordinator.restart("/tmp/update-ready.tar.gz", false);
    assert!(!restart_success);

    assert_eq!(
        coordinator.state(),
        UpdateState::RestartFailed {
            downloaded_path: "/tmp/update-ready.tar.gz".into(),
        }
    );
}

type MockCoordinator = UpdateCoordinator<MockSource, MockFiles, MockInstaller>;
/// Every state the coordinator published, in order.
type PublishedStates = Arc<Mutex<Vec<UpdateState>>>;

fn recording_coordinator(source: MockSource) -> (MockCoordinator, PublishedStates) {
    let published = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&published);
    let coordinator = UpdateCoordinator::new(
        source,
        MockFiles::default(),
        MockInstaller::default(),
        move |state| recorder.lock().unwrap().push(state.clone()),
    );
    (coordinator, published)
}

#[tokio::test]
async fn a_failed_check_publishes_check_failed_with_the_source_detail_and_nothing_else() {
    let source = MockSource::default();
    *source.check_error.lock().unwrap() = Some("HTTP error: 503 Service Unavailable".into());
    let (coordinator, published) = recording_coordinator(source);

    coordinator.check_for_updates().await;

    let expected = UpdateState::CheckFailed("HTTP error: 503 Service Unavailable".into());
    assert_eq!(
        *published.lock().unwrap(),
        vec![UpdateState::Checking, expected.clone()]
    );
    assert_eq!(coordinator.state(), expected);
    // A failed check leaves the updater free to try again.
    assert!(coordinator.can_check());
}

#[tokio::test]
async fn a_failed_check_after_a_found_release_reports_the_failure_and_a_later_empty_check_is_idle()
{
    let source = MockSource::default();
    let info = sample_update_info();
    *source.next_info.lock().unwrap() = Some(info.clone());
    let (coordinator, published) = recording_coordinator(source.clone());

    coordinator.check_for_updates().await;
    *source.check_error.lock().unwrap() = Some("Network error: connection refused".into());
    coordinator.check_for_updates().await;
    *source.check_error.lock().unwrap() = None;
    *source.next_info.lock().unwrap() = None;
    coordinator.check_for_updates().await;

    assert_eq!(
        *published.lock().unwrap(),
        vec![
            UpdateState::Checking,
            UpdateState::Available(info),
            UpdateState::Checking,
            UpdateState::CheckFailed("Network error: connection refused".into()),
            UpdateState::Checking,
            UpdateState::Idle,
        ]
    );
}

#[tokio::test]
async fn each_repeated_failed_check_is_published_again() {
    let source = MockSource::default();
    *source.check_error.lock().unwrap() = Some("Network error: timed out".into());
    let (coordinator, published) = recording_coordinator(source);

    coordinator.check_for_updates().await;
    coordinator.check_for_updates().await;

    let failed = UpdateState::CheckFailed("Network error: timed out".into());
    assert_eq!(
        *published.lock().unwrap(),
        vec![
            UpdateState::Checking,
            failed.clone(),
            UpdateState::Checking,
            failed,
        ]
    );
}

#[tokio::test]
async fn a_failed_check_superseded_by_a_newer_check_does_not_overwrite_its_result() {
    let source = MockSource::default();
    let (check_start_tx, check_start_rx) = tokio::sync::oneshot::channel();
    let (check_resume_tx, check_resume_rx) = tokio::sync::oneshot::channel();
    *source.check_started.lock().unwrap() = Some(check_start_tx);
    *source.check_resume.lock().unwrap() = Some(check_resume_rx);
    let info = sample_update_info();
    *source.next_info.lock().unwrap() = Some(info.clone());
    let (coordinator, published) = recording_coordinator(source.clone());
    let coordinator = Arc::new(coordinator);

    let older_check = tokio::spawn({
        let coordinator = Arc::clone(&coordinator);
        async move { coordinator.check_for_updates().await }
    });
    check_started_within_limit(check_start_rx).await;

    // The newer check finds the release while the older one is still waiting.
    within_harness_limit("the newer check", coordinator.check_for_updates()).await;
    // The older one then fails.
    *source.check_error.lock().unwrap() = Some("Network error: reset".into());
    check_resume_tx
        .send(())
        .expect("the older check is waiting");
    within_harness_limit("the older check", older_check)
        .await
        .expect("the older check must finish");

    assert_eq!(coordinator.state(), UpdateState::Available(info.clone()));
    assert_eq!(
        *published.lock().unwrap(),
        vec![UpdateState::Checking, UpdateState::Available(info)]
    );
}

#[tokio::test]
async fn a_check_that_fails_while_a_download_finishes_leaves_the_update_ready_to_restart() {
    let source = MockSource::default();
    let (check_start_tx, check_start_rx) = tokio::sync::oneshot::channel();
    let (check_resume_tx, check_resume_rx) = tokio::sync::oneshot::channel();
    *source.check_started.lock().unwrap() = Some(check_start_tx);
    *source.check_resume.lock().unwrap() = Some(check_resume_rx);
    *source.check_error.lock().unwrap() = Some("Network error: reset".into());
    let files = MockFiles::default();
    *files.destination.lock().unwrap() = "/tmp/ready-after-failed-check.AppImage".into();
    let published = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&published);
    let coordinator = Arc::new(UpdateCoordinator::new(
        source,
        files,
        MockInstaller::default(),
        move |state| recorder.lock().unwrap().push(state.clone()),
    ));

    let check = tokio::spawn({
        let coordinator = Arc::clone(&coordinator);
        async move { coordinator.check_for_updates().await }
    });
    check_started_within_limit(check_start_rx).await;
    let info = sample_update_info();
    within_harness_limit(
        "a download during a pending check",
        coordinator.download(&info),
    )
    .await;
    check_resume_tx.send(()).expect("the check is waiting");
    within_harness_limit("the failing check", check)
        .await
        .expect("the check must finish");

    let ready = UpdateState::ReadyToRestart {
        downloaded_path: "/tmp/ready-after-failed-check.AppImage".into(),
        is_steam_game_mode: false,
    };
    assert_eq!(coordinator.state(), ready);
    let published = published.lock().unwrap().clone();
    assert_eq!(published.last(), Some(&ready), "{published:?}");
    assert!(
        !published
            .iter()
            .any(|state| matches!(state, UpdateState::CheckFailed(_))),
        "the failed check is stale once a download started: {published:?}"
    );
}

/// A coordinator over the given fakes that records every state it publishes.
fn recording_coordinator_over(
    source: MockSource,
    files: MockFiles,
    installer: MockInstaller,
) -> (MockCoordinator, PublishedStates) {
    let published = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&published);
    let coordinator = UpdateCoordinator::new(source, files, installer, move |state| {
        recorder.lock().unwrap().push(state.clone())
    });
    (coordinator, published)
}

/// The published states other than download progress, in order.
fn outcomes(published: &PublishedStates) -> Vec<UpdateState> {
    published
        .lock()
        .unwrap()
        .iter()
        .filter(|state| !matches!(state, UpdateState::Downloading { .. }))
        .cloned()
        .collect()
}

#[tokio::test]
async fn a_destination_that_cannot_be_prepared_is_reported_as_such_and_deletes_nothing() {
    let files = MockFiles::default();
    *files.prepare_error.lock().unwrap() = Some("Permission denied (os error 13)".into());
    let (coordinator, published) = recording_coordinator_over(
        MockSource::default(),
        files.clone(),
        MockInstaller::default(),
    );
    let info = sample_update_info();

    within_harness_limit("the download", coordinator.download(&info)).await;

    assert_eq!(
        *published.lock().unwrap(),
        vec![
            UpdateState::Downloading {
                progress: 0.0,
                info: info.clone(),
            },
            UpdateState::DownloadFailed(DownloadFailure::DestinationUnavailable(
                "Permission denied (os error 13)".into()
            )),
        ]
    );
    assert_eq!(
        *files.deleted_paths.lock().unwrap(),
        Vec::<String>::new(),
        "no destination was prepared, so there is nothing to delete"
    );
    assert!(
        coordinator.can_check(),
        "the failed download frees the updater"
    );
}

#[tokio::test]
async fn an_unreadable_download_size_is_reported_with_its_detail_and_deletes_the_download() {
    let files = MockFiles::default();
    *files.destination.lock().unwrap() = "/tmp/unreadable-size.AppImage".into();
    *files.size_error.lock().unwrap() = Some("No such file or directory (os error 2)".into());
    let (coordinator, published) = recording_coordinator_over(
        MockSource::default(),
        files.clone(),
        MockInstaller::default(),
    );

    within_harness_limit("the download", coordinator.download(&sample_update_info())).await;

    let failed = UpdateState::DownloadFailed(DownloadFailure::SizeUnreadable(
        "No such file or directory (os error 2)".into(),
    ));
    assert_eq!(outcomes(&published), vec![failed.clone()]);
    assert_eq!(coordinator.state(), failed);
    assert_eq!(
        *files.deleted_paths.lock().unwrap(),
        vec!["/tmp/unreadable-size.AppImage".to_string()],
        "an unverified download must not stay on disk"
    );
    assert!(coordinator.can_check());
}

#[tokio::test]
async fn downloads_after_failed_ones_report_each_failure_and_then_end_ready_to_restart() {
    let source = MockSource::default();
    *source.download_error.lock().unwrap() = Some("Network cut".into());
    let files = MockFiles::default();
    *files.destination.lock().unwrap() = "/tmp/retried-update.AppImage".into();
    let (coordinator, published) =
        recording_coordinator_over(source.clone(), files.clone(), MockInstaller::default());
    let info = sample_update_info();

    within_harness_limit("the first download", coordinator.download(&info)).await;
    within_harness_limit("the second download", coordinator.download(&info)).await;
    *source.download_error.lock().unwrap() = None;
    within_harness_limit("the third download", coordinator.download(&info)).await;

    let failed = UpdateState::DownloadFailed(DownloadFailure::Transfer("Network cut".into()));
    let ready = UpdateState::ReadyToRestart {
        downloaded_path: "/tmp/retried-update.AppImage".into(),
        is_steam_game_mode: false,
    };
    assert_eq!(
        outcomes(&published),
        vec![failed.clone(), failed, ready.clone()],
        "a repeated failure is published again, so the dialog never stays on the progress bar"
    );
    let published = published.lock().unwrap().clone();
    let first_failure = published
        .iter()
        .position(|state| matches!(state, UpdateState::DownloadFailed(_)))
        .expect("the first download failed");
    assert!(
        matches!(
            published[first_failure + 1],
            UpdateState::Downloading { progress, .. } if progress == 0.0
        ),
        "the next download starts from zero after a failure: {published:?}"
    );
    assert_eq!(
        *files.deleted_paths.lock().unwrap(),
        vec![
            "/tmp/retried-update.AppImage".to_string(),
            "/tmp/retried-update.AppImage".to_string(),
        ],
        "each failed download is deleted and the successful one is kept"
    );
    assert_eq!(coordinator.state(), ready);
}

#[tokio::test]
async fn a_refused_or_failed_restart_keeps_the_downloaded_file_and_reports_its_path() {
    for restart_result in [Err("Text file busy (os error 26)".to_string()), Ok(false)] {
        let files = MockFiles::default();
        *files.destination.lock().unwrap() = "/tmp/kept-update.AppImage".into();
        let installer = MockInstaller::default();
        let (coordinator, published) =
            recording_coordinator_over(MockSource::default(), files.clone(), installer.clone());
        within_harness_limit("the download", coordinator.download(&sample_update_info())).await;
        *installer.restart_result.lock().unwrap() = Some(restart_result.clone());

        let is_applied = coordinator.restart("/tmp/kept-update.AppImage", false);

        assert!(!is_applied, "{restart_result:?}");
        let failed = UpdateState::RestartFailed {
            downloaded_path: "/tmp/kept-update.AppImage".into(),
        };
        assert_eq!(coordinator.state(), failed, "{restart_result:?}");
        assert_eq!(
            published.lock().unwrap().last(),
            Some(&failed),
            "{restart_result:?}: the interface learns of the failure"
        );
        assert_eq!(
            *files.deleted_paths.lock().unwrap(),
            Vec::<String>::new(),
            "{restart_result:?}: the user installs the kept file by hand"
        );
    }
}

#[tokio::test]
async fn after_a_failed_restart_a_new_check_and_download_run_again() {
    let source = MockSource::default();
    let files = MockFiles::default();
    *files.destination.lock().unwrap() = "/tmp/after-restart-failure.AppImage".into();
    let installer = MockInstaller::default();
    *installer.restart_result.lock().unwrap() = Some(Ok(false));
    let (coordinator, published) = recording_coordinator_over(source.clone(), files, installer);
    let info = sample_update_info();
    within_harness_limit("the first download", coordinator.download(&info)).await;
    assert!(!coordinator.restart("/tmp/after-restart-failure.AppImage", false));
    assert!(
        coordinator.can_check(),
        "the header offers a new check after a failed restart"
    );
    published.lock().unwrap().clear();

    *source.next_info.lock().unwrap() = Some(info.clone());
    within_harness_limit("the check", coordinator.check_for_updates()).await;
    within_harness_limit("the second download", coordinator.download(&info)).await;

    assert_eq!(
        outcomes(&published),
        vec![
            UpdateState::Checking,
            UpdateState::Available(info),
            UpdateState::ReadyToRestart {
                downloaded_path: "/tmp/after-restart-failure.AppImage".into(),
                is_steam_game_mode: false,
            },
        ]
    );
}

type ReactingCoordinator = UpdateCoordinator<MockSource, MockFiles, MockInstaller>;

/// A coordinator that hands each announced state to `react` together with itself, as an
/// interface that acts on a state as soon as it arrives.
fn coordinator_reacting(
    source: MockSource,
    files: MockFiles,
    installer: MockInstaller,
    react: impl Fn(&ReactingCoordinator, &UpdateState) + Send + Sync + 'static,
) -> Arc<ReactingCoordinator> {
    Arc::new_cyclic(|this: &std::sync::Weak<ReactingCoordinator>| {
        let this = this.clone();
        UpdateCoordinator::new(source, files, installer, move |state: &UpdateState| {
            if let Some(coordinator) = this.upgrade() {
                react(&coordinator, state);
            }
        })
    })
}

/// Runs `coordinator.download(info)` on a thread of its own and waits for it with a
/// wall-clock limit: a reaction blocked on the coordinator's lock (a state announced
/// while it is held) then fails the test instead of hanging the test's only thread.
fn download_reacting_within_harness_limit(
    coordinator: &Arc<ReactingCoordinator>,
    info: &UpdateInfo,
) {
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let coordinator = Arc::clone(coordinator);
    let info = info.clone();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("build the download's runtime");
        runtime.block_on(coordinator.download(&info));
        let _ = finished_tx.send(());
    });
    finished_rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("the download did not finish within the harness limit");
}

/// What an observer of the failure finds when it is announced.
#[derive(Debug, PartialEq)]
struct FailureAnnouncement {
    can_check: bool,
    deleted_paths: Vec<String>,
}

#[tokio::test]
async fn when_a_download_failure_is_announced_the_file_is_gone_and_the_updater_is_free() {
    let source = MockSource::default();
    *source.download_error.lock().unwrap() = Some("Network cut".into());
    let files = MockFiles::default();
    *files.destination.lock().unwrap() = "/tmp/announced-failure.AppImage".into();
    let announcements: Arc<Mutex<Vec<FailureAnnouncement>>> = Arc::default();
    let coordinator = coordinator_reacting(source, files.clone(), MockInstaller::default(), {
        let announcements = Arc::clone(&announcements);
        let files = files.clone();
        move |coordinator, state| {
            if matches!(state, UpdateState::DownloadFailed(_)) {
                announcements.lock().unwrap().push(FailureAnnouncement {
                    can_check: coordinator.can_check(),
                    deleted_paths: files.deleted_paths.lock().unwrap().clone(),
                });
            }
        }
    });

    download_reacting_within_harness_limit(&coordinator, &sample_update_info());

    // The interface reacts to the announced state (the header then offers a new check,
    // the user may retry): an updater still busy at that moment would ignore it.
    assert_eq!(
        *announcements.lock().unwrap(),
        vec![FailureAnnouncement {
            can_check: true,
            deleted_paths: vec!["/tmp/announced-failure.AppImage".to_string()],
        }]
    );
}

#[tokio::test]
async fn an_update_announced_ready_to_restart_can_be_restarted_at_once() {
    let files = MockFiles::default();
    *files.destination.lock().unwrap() = "/tmp/announced-ready.AppImage".into();
    let installer = MockInstaller::default();
    let restarts: Arc<Mutex<Vec<bool>>> = Arc::default();
    let coordinator =
        coordinator_reacting(MockSource::default(), files.clone(), installer.clone(), {
            let restarts = Arc::clone(&restarts);
            move |coordinator, state| {
                if let UpdateState::ReadyToRestart {
                    downloaded_path, ..
                } = state
                {
                    restarts
                        .lock()
                        .unwrap()
                        .push(coordinator.restart(downloaded_path, false));
                }
            }
        });

    download_reacting_within_harness_limit(&coordinator, &sample_update_info());

    // "Restart Now" pressed as soon as it shows must not be refused as a running download.
    assert_eq!(*restarts.lock().unwrap(), vec![true]);
    assert_eq!(
        installer.restarted_path.lock().unwrap().as_deref(),
        Some("/tmp/announced-ready.AppImage")
    );
    assert_eq!(*files.deleted_paths.lock().unwrap(), Vec::<String>::new());
}

/// Polls `step` once, as a reaction inside `on_state` gets to run it before returning.
fn poll_once<T>(
    step: std::pin::Pin<&mut impl std::future::Future<Output = T>>,
) -> std::task::Poll<T> {
    step.poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
}

/// Runs `step` to its end inside the current call; the fakes never wait, so a step that
/// has to wait is a failure.
fn finish_without_waiting<T>(step: impl std::future::Future<Output = T>) -> T {
    let mut step = std::pin::pin!(step);
    match poll_once(step.as_mut()) {
        std::task::Poll::Ready(output) => output,
        std::task::Poll::Pending => panic!("the step had to wait"),
    }
}

fn downloading(progress: f32, info: &UpdateInfo) -> UpdateState {
    UpdateState::Downloading {
        progress,
        info: info.clone(),
    }
}

#[tokio::test]
async fn a_retry_started_when_a_download_failure_is_announced_runs_and_ends_ready_to_restart() {
    let source = MockSource::default();
    *source.download_error.lock().unwrap() = Some("Network cut".into());
    let files = MockFiles::default();
    *files.destination.lock().unwrap() = "/tmp/retried-at-once.AppImage".into();
    let published: PublishedStates = Arc::default();
    let coordinator =
        coordinator_reacting(source.clone(), files.clone(), MockInstaller::default(), {
            let published = Arc::clone(&published);
            move |coordinator, state| {
                published.lock().unwrap().push(state.clone());
                if matches!(state, UpdateState::DownloadFailed(_)) {
                    // "Retry" pressed the moment the failure shows, with the network back.
                    *source.download_error.lock().unwrap() = None;
                    finish_without_waiting(coordinator.download(&sample_update_info()));
                }
            }
        });
    let info = sample_update_info();

    download_reacting_within_harness_limit(&coordinator, &info);

    let ready = UpdateState::ReadyToRestart {
        downloaded_path: "/tmp/retried-at-once.AppImage".into(),
        is_steam_game_mode: false,
    };
    assert_eq!(
        *published.lock().unwrap(),
        vec![
            downloading(0.0, &info),
            downloading(0.5, &info),
            UpdateState::DownloadFailed(DownloadFailure::Transfer("Network cut".into())),
            downloading(0.0, &info),
            downloading(0.5, &info),
            downloading(1.0, &info),
            ready.clone(),
        ],
        "a retry ignored as a running download would leave the dialog on the failure"
    );
    assert_eq!(coordinator.state(), ready);
    assert_eq!(
        *files.deleted_paths.lock().unwrap(),
        vec!["/tmp/retried-at-once.AppImage".to_string()],
        "only the failed attempt is deleted; the retried file is kept"
    );
}

#[tokio::test]
async fn a_check_started_when_a_download_failure_is_announced_publishes_its_result() {
    let source = MockSource::default();
    *source.download_error.lock().unwrap() = Some("Network cut".into());
    let info = sample_update_info();
    *source.next_info.lock().unwrap() = Some(info.clone());
    let files = MockFiles::default();
    *files.destination.lock().unwrap() = "/tmp/checked-at-once.AppImage".into();
    let published: PublishedStates = Arc::default();
    let coordinator = coordinator_reacting(source, files, MockInstaller::default(), {
        let published = Arc::clone(&published);
        move |coordinator, state| {
            published.lock().unwrap().push(state.clone());
            if matches!(state, UpdateState::DownloadFailed(_)) {
                // The header offers a new check as soon as the failure shows.
                finish_without_waiting(coordinator.check_for_updates());
            }
        }
    });

    download_reacting_within_harness_limit(&coordinator, &info);

    assert_eq!(
        *published.lock().unwrap(),
        vec![
            downloading(0.0, &info),
            downloading(0.5, &info),
            UpdateState::DownloadFailed(DownloadFailure::Transfer("Network cut".into())),
            UpdateState::Checking,
            UpdateState::Available(info.clone()),
        ]
    );
    assert_eq!(
        coordinator.state(),
        UpdateState::Available(info),
        "the end of the failed download must not overwrite the newer check"
    );
    assert!(coordinator.can_check());
}

#[tokio::test]
async fn each_download_outcome_is_published_once_after_its_progress_and_nothing_follows() {
    let info = sample_update_info();
    let transfer_failure = MockSource::default();
    *transfer_failure.download_error.lock().unwrap() = Some("Network cut".into());
    let wrong_size = MockFiles::default();
    *wrong_size.custom_size.lock().unwrap() = Some(2048);
    let cases = [
        (
            "transfer failure",
            transfer_failure,
            MockFiles::default(),
            vec![
                downloading(0.0, &info),
                downloading(0.5, &info),
                UpdateState::DownloadFailed(DownloadFailure::Transfer("Network cut".into())),
            ],
        ),
        (
            "size mismatch",
            MockSource::default(),
            wrong_size,
            vec![
                downloading(0.0, &info),
                downloading(0.5, &info),
                downloading(1.0, &info),
                UpdateState::DownloadFailed(DownloadFailure::SizeMismatch {
                    expected_bytes: 1024,
                    received_bytes: 2048,
                }),
            ],
        ),
        (
            "success",
            MockSource::default(),
            MockFiles::default(),
            vec![
                downloading(0.0, &info),
                downloading(0.5, &info),
                downloading(1.0, &info),
                UpdateState::ReadyToRestart {
                    downloaded_path: "/tmp/exact-sequence.AppImage".into(),
                    is_steam_game_mode: false,
                },
            ],
        ),
    ];
    for (case, source, files, expected) in cases {
        *files.destination.lock().unwrap() = "/tmp/exact-sequence.AppImage".into();
        let (coordinator, published) =
            recording_coordinator_over(source, files, MockInstaller::default());

        within_harness_limit("the download", coordinator.download(&info)).await;

        assert_eq!(*published.lock().unwrap(), expected, "{case}");
        assert_eq!(
            Some(coordinator.state()),
            expected.last().cloned(),
            "{case}"
        );
    }
}

/// What an observer finds when a cancelled download announces the release again.
#[derive(Debug, PartialEq)]
struct CancellationAnnouncement {
    state: UpdateState,
    can_check: bool,
    deleted_paths: Vec<String>,
}

#[tokio::test]
async fn a_download_cancelled_at_each_await_offers_the_release_again_with_the_updater_free() {
    let info = sample_update_info();
    let stalled_preparation = MockFiles::default();
    *stalled_preparation.stall_prepare.lock().unwrap() = true;
    let stalled_transfer = MockSource::default();
    *stalled_transfer.stall_download.lock().unwrap() = true;
    let cases = [
        (
            "before the destination is ready",
            MockSource::default(),
            stalled_preparation,
            vec![
                downloading(0.0, &info),
                UpdateState::Available(info.clone()),
            ],
            Vec::<String>::new(),
        ),
        (
            "during the transfer",
            stalled_transfer,
            MockFiles::default(),
            vec![
                downloading(0.0, &info),
                downloading(0.5, &info),
                UpdateState::Available(info.clone()),
            ],
            vec!["/tmp/cancelled.AppImage".to_string()],
        ),
    ];
    for (case, source, files, expected_published, expected_deleted) in cases {
        *files.destination.lock().unwrap() = "/tmp/cancelled.AppImage".into();
        let published: PublishedStates = Arc::default();
        let announcements: Arc<Mutex<Vec<CancellationAnnouncement>>> = Arc::default();
        let coordinator =
            coordinator_reacting(source.clone(), files.clone(), MockInstaller::default(), {
                let published = Arc::clone(&published);
                let announcements = Arc::clone(&announcements);
                let files = files.clone();
                move |coordinator, state| {
                    published.lock().unwrap().push(state.clone());
                    if matches!(state, UpdateState::Available(_)) {
                        announcements
                            .lock()
                            .unwrap()
                            .push(CancellationAnnouncement {
                                state: coordinator.state(),
                                can_check: coordinator.can_check(),
                                deleted_paths: files.deleted_paths.lock().unwrap().clone(),
                            });
                    }
                }
            });

        let mut download = Box::pin(coordinator.download(&info));
        assert!(poll_once(download.as_mut()).is_pending(), "{case}");
        assert!(!coordinator.can_check(), "{case}: the download is running");
        drop(download);

        assert_eq!(*published.lock().unwrap(), expected_published, "{case}");
        assert_eq!(
            *announcements.lock().unwrap(),
            vec![CancellationAnnouncement {
                state: UpdateState::Available(info.clone()),
                can_check: true,
                deleted_paths: expected_deleted.clone(),
            }],
            "{case}"
        );
        assert_eq!(
            coordinator.state(),
            UpdateState::Available(info.clone()),
            "{case}"
        );

        // The release offered again can be downloaded at once.
        *files.stall_prepare.lock().unwrap() = false;
        *source.stall_download.lock().unwrap() = false;
        within_harness_limit("the next download", coordinator.download(&info)).await;
        assert_eq!(
            coordinator.state(),
            UpdateState::ReadyToRestart {
                downloaded_path: "/tmp/cancelled.AppImage".into(),
                is_steam_game_mode: false,
            },
            "{case}"
        );
        assert_eq!(
            *files.deleted_paths.lock().unwrap(),
            expected_deleted,
            "{case}"
        );
    }
}

#[tokio::test]
async fn a_pending_check_that_ends_after_a_failed_download_does_not_overwrite_the_failure() {
    for is_check_cancelled in [false, true] {
        let source = MockSource::default();
        let (check_start_tx, check_start_rx) = tokio::sync::oneshot::channel();
        let (check_resume_tx, check_resume_rx) = tokio::sync::oneshot::channel();
        *source.check_started.lock().unwrap() = Some(check_start_tx);
        *source.check_resume.lock().unwrap() = Some(check_resume_rx);
        let info = sample_update_info();
        *source.next_info.lock().unwrap() = Some(info.clone());
        *source.download_error.lock().unwrap() = Some("Network cut".into());
        let files = MockFiles::default();
        *files.destination.lock().unwrap() = "/tmp/failure-before-check.AppImage".into();
        let (coordinator, published) =
            recording_coordinator_over(source, files, MockInstaller::default());
        let coordinator = Arc::new(coordinator);

        let check = tokio::spawn({
            let coordinator = Arc::clone(&coordinator);
            async move { coordinator.check_for_updates().await }
        });
        check_started_within_limit(check_start_rx).await;
        within_harness_limit("the download", coordinator.download(&info)).await;
        if is_check_cancelled {
            check.abort();
            let _ = check.await;
        } else {
            check_resume_tx.send(()).expect("the check is waiting");
            within_harness_limit("the stale check", check)
                .await
                .expect("the check must finish");
        }

        let failed = UpdateState::DownloadFailed(DownloadFailure::Transfer("Network cut".into()));
        assert_eq!(
            coordinator.state(),
            failed,
            "cancelled: {is_check_cancelled}"
        );
        assert_eq!(
            *published.lock().unwrap(),
            vec![
                UpdateState::Checking,
                downloading(0.0, &info),
                downloading(0.5, &info),
                failed,
            ],
            "cancelled: {is_check_cancelled}"
        );
        assert!(coordinator.can_check(), "cancelled: {is_check_cancelled}");
    }
}
