use emusync_application::{UpdateCoordinator, UpdateState};
use emusync_domain::update::{UpdateFiles, UpdateInfo, UpdateInstaller, UpdateSource};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct MockSource {
    next_info: Arc<Mutex<Option<UpdateInfo>>>,
    check_error: Arc<Mutex<Option<String>>>,
    download_error: Arc<Mutex<Option<String>>>,
}

impl UpdateSource for MockSource {
    type Error = String;

    async fn check(&self) -> Result<Option<UpdateInfo>, Self::Error> {
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
}

impl UpdateFiles for MockFiles {
    type Error = String;

    async fn prepare_destination(&self, _info: &UpdateInfo) -> Result<String, Self::Error> {
        Ok(self.destination.lock().unwrap().clone())
    }

    fn size(&self, _path: &str) -> Result<u64, Self::Error> {
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
}

impl UpdateInstaller for MockInstaller {
    type Error = String;

    fn is_steam_game_mode(&self) -> Result<bool, Self::Error> {
        Ok(false)
    }

    fn restart(&self, path: &str) -> Result<bool, Self::Error> {
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
