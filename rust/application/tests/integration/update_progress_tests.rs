//! The progress `UpdateCoordinator` publishes never reaches 100 % while downloading:
//! the coordinator still checks the size after the source reports completion, so only
//! `ReadyToRestart` means complete. A download that ends rejected must never have been
//! shown as complete, and a valid one ends at 99 % and then `ReadyToRestart`.

use emusync_application::{DownloadFailure, UpdateCoordinator, UpdateState};
use emusync_domain::update::{UpdateFiles, UpdateInfo, UpdateInstaller, UpdateSource};
use std::sync::{Arc, Mutex};

/// The lowest progress the interface shows as «100%»: it shows whole percentages,
/// rounded.
const LOWEST_PROGRESS_SHOWN_AS_ONE_HUNDRED_PERCENT: f32 = 0.995;

const DESTINATION: &str = "/tmp/progress-contract.AppImage";

/// Reports each value of `reported_progress` as the downloader would, then ends with
/// `outcome`.
struct ScriptedSource {
    reported_progress: Vec<f32>,
    outcome: Result<(), String>,
}

impl UpdateSource for ScriptedSource {
    type Error = String;

    async fn check(&self) -> Result<Option<UpdateInfo>, Self::Error> {
        Ok(None)
    }

    async fn download(
        &self,
        _info: &UpdateInfo,
        _destination: &str,
        mut on_progress: impl FnMut(f32) + Send,
    ) -> Result<(), Self::Error> {
        for progress in &self.reported_progress {
            on_progress(*progress);
        }
        self.outcome.clone()
    }
}

/// A destination whose downloaded size is `downloaded_size_bytes`.
struct SizedFiles {
    downloaded_size_bytes: u64,
}

impl UpdateFiles for SizedFiles {
    type Error = String;

    async fn prepare_destination(&self, _info: &UpdateInfo) -> Result<String, Self::Error> {
        Ok(DESTINATION.to_owned())
    }

    fn size(&self, _path: &str) -> Result<u64, Self::Error> {
        Ok(self.downloaded_size_bytes)
    }

    fn delete(&self, _path: &str) -> Result<(), Self::Error> {
        Ok(())
    }
}

struct DesktopInstaller;

impl UpdateInstaller for DesktopInstaller {
    type Error = String;

    fn is_steam_game_mode(&self) -> Result<bool, Self::Error> {
        Ok(false)
    }

    fn restart(&self, _path: &str) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

fn release_of_1024_bytes() -> UpdateInfo {
    UpdateInfo {
        version: "2.0.0".into(),
        tag_name: "v2.0.0".into(),
        release_notes: "New release".into(),
        download_url: "https://example.com/EmuSync-x86_64.AppImage".into(),
        asset_size: 1024,
        asset_name: "EmuSync-x86_64.AppImage".into(),
        manifest_url: String::new(),
        signature_url: String::new(),
    }
}

/// Every state the coordinator publishes while downloading the 1024-byte release from
/// `source` into a file of `downloaded_size_bytes`.
async fn published_states(source: ScriptedSource, downloaded_size_bytes: u64) -> Vec<UpdateState> {
    let published = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&published);
    let coordinator = UpdateCoordinator::new(
        source,
        SizedFiles {
            downloaded_size_bytes,
        },
        DesktopInstaller,
        move |state: &UpdateState| recorder.lock().unwrap().push(state.clone()),
    );
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        coordinator.download(&release_of_1024_bytes()),
    )
    .await
    .expect("the download must finish");
    let published = published.lock().unwrap().clone();
    assert_eq!(
        Some(coordinator.state()),
        published.last().cloned(),
        "the last published state is the coordinator's state"
    );
    published
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

#[tokio::test]
async fn a_size_mismatch_found_after_the_source_reports_completion_was_never_shown_as_complete() {
    // What the real downloader reports for a file whose signed hash matches.
    let source = ScriptedSource {
        reported_progress: vec![0.5, 0.99, 1.0],
        outcome: Ok(()),
    };

    let states = published_states(source, 2048).await;

    assert_eq!(
        states.last(),
        Some(&UpdateState::DownloadFailed(
            DownloadFailure::SizeMismatch {
                expected_bytes: 1024,
                received_bytes: 2048,
            }
        )),
        "{states:?}"
    );
    assert_eq!(
        shown_as_complete(&states),
        Vec::<f32>::new(),
        "a rejected update was shown at 100 %: {states:?}"
    );
}

#[tokio::test]
async fn a_source_that_fails_after_reporting_completion_was_never_shown_as_complete() {
    let source = ScriptedSource {
        reported_progress: vec![0.99, 1.0],
        outcome: Err("could not move the download into place".into()),
    };

    let states = published_states(source, 1024).await;

    assert_eq!(
        states.last(),
        Some(&UpdateState::DownloadFailed(DownloadFailure::Transfer(
            "could not move the download into place".into()
        ))),
        "{states:?}"
    );
    assert_eq!(
        shown_as_complete(&states),
        Vec::<f32>::new(),
        "a failed update was shown at 100 %: {states:?}"
    );
}

#[tokio::test]
async fn progress_above_one_from_the_source_is_never_shown_as_complete_before_the_size_check() {
    let source = ScriptedSource {
        reported_progress: vec![0.5, 1.7, f32::INFINITY, 1.0],
        outcome: Ok(()),
    };

    let states = published_states(source, 2048).await;

    assert!(
        matches!(
            states.last(),
            Some(UpdateState::DownloadFailed(
                DownloadFailure::SizeMismatch { .. }
            ))
        ),
        "{states:?}"
    );
    assert_eq!(
        shown_as_complete(&states),
        Vec::<f32>::new(),
        "a rejected update was shown at 100 %: {states:?}"
    );
}

#[tokio::test]
async fn a_valid_download_ends_at_ninety_nine_percent_and_then_ready_to_restart() {
    let source = ScriptedSource {
        reported_progress: vec![0.5, 0.99, 1.0],
        outcome: Ok(()),
    };

    let states = published_states(source, 1024).await;

    assert_eq!(
        shown_as_complete(&states),
        Vec::<f32>::new(),
        "only ReadyToRestart means complete: {states:?}"
    );
    let info = release_of_1024_bytes();
    assert_eq!(
        states[states.len().saturating_sub(2)..],
        [
            UpdateState::Downloading {
                progress: 0.99,
                info,
            },
            UpdateState::ReadyToRestart {
                downloaded_path: DESTINATION.into(),
                is_steam_game_mode: false,
            },
        ],
        "{states:?}"
    );
}

#[tokio::test]
async fn a_valid_download_whose_last_step_is_under_one_percent_still_ends_at_ninety_nine_percent() {
    // The bytes arrive at 98.5 %, then the rest: the downloader reports its cap and then
    // completion, each less than one point above the last published progress.
    let source = ScriptedSource {
        reported_progress: vec![0.985, 0.99, 1.0],
        outcome: Ok(()),
    };

    let states = published_states(source, 1024).await;

    assert_eq!(
        shown_as_complete(&states),
        Vec::<f32>::new(),
        "only ReadyToRestart means complete: {states:?}"
    );
    let info = release_of_1024_bytes();
    assert_eq!(
        states[states.len().saturating_sub(2)..],
        [
            UpdateState::Downloading {
                progress: 0.99,
                info,
            },
            UpdateState::ReadyToRestart {
                downloaded_path: DESTINATION.into(),
                is_steam_game_mode: false,
            },
        ],
        "{states:?}"
    );
}
