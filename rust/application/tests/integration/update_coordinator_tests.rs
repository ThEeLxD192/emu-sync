use emusync_application::{UpdateCoordinator, UpdateState};
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
}

impl UpdateFiles for MockFiles {
    type Error = String;

    async fn prepare_destination(&self, _info: &UpdateInfo) -> Result<String, Self::Error> {
        Ok(self.destination.lock().unwrap().clone())
    }

    fn size(&self, _path: &str) -> Result<u64, Self::Error> {
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
    }
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

    // Error state published
    let states = published.lock().unwrap().clone();
    assert!(states.iter().any(|s| matches!(s, UpdateState::Error(_))));
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
    rx.await.expect("download should have started");

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

    // Verify UpdateState::Error with expected size mismatch message
    let current_state = coordinator.state();
    match current_state {
        UpdateState::Error(msg) => {
            assert!(
                msg.contains("expected 4096 bytes, received 2048"),
                "Error message must specify expected vs received size, got: {msg}"
            );
        }
        other => panic!("Expected UpdateState::Error, got: {other:?}"),
    }

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
    check_start_rx.await.expect("check must have started");

    // Perform download while check is pending, transitioning state to ReadyToRestart
    coordinator.download(&info).await;
    assert!(matches!(
        coordinator.state(),
        UpdateState::ReadyToRestart { .. }
    ));

    // Resume the stale check task
    check_resume_tx.send(()).expect("check_resume send failed");
    check_task.await.expect("check_task join failed");

    // Check must NOT overwrite ReadyToRestart with Available
    assert!(
        matches!(coordinator.state(), UpdateState::ReadyToRestart { .. }),
        "Stale check must not overwrite ReadyToRestart"
    );

    // Test failed restart publishes error state
    *installer.restart_result.lock().unwrap() = Some(Ok(false));
    let restart_success = coordinator.restart("/tmp/update-ready.tar.gz", false);
    assert!(!restart_success);

    match coordinator.state() {
        UpdateState::Error(msg) => {
            assert!(
                msg.contains("Could not restart automatically. The update file is saved at: /tmp/update-ready.tar.gz"),
                "Error must report restart failure and saved update path, got: {msg}"
            );
        }
        other => panic!("Expected UpdateState::Error, got: {other:?}"),
    }
}
