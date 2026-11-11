//! A save folder's excluded folders never reach the cloud, never come back from it and
//! never make the game look out of sync.

mod common;

use common::*;
use emusync_domain::library::{SaveTarget, SaveTargetType};
use emusync_domain::sync::{Decision, EntrySyncStatus, SyncGame};

const ROOT: &str = "/saves/ctr";
const KEPT: &str = "/saves/ctr/1/CUSA/savedata0/SYS-DATA.sav";
const REPLAY: &str = "/saves/ctr/1/CUSA/RaceReplay/race1.rep";
/// Shares the excluded folder's name as a prefix but is a different folder.
const LOOKALIKE: &str = "/saves/ctr/1/CUSA/RaceReplayIndex/index.dat";

fn ctr_game() -> SyncGame {
    let mut game = sample_native_game("CTR", ROOT);
    let target = SaveTarget {
        path: ROOT.into(),
        kind: SaveTargetType::Directory,
        cloud_path: Some("Savedata".into()),
        excluded_folders: vec!["1/CUSA/RaceReplay".into()],
    };
    game.save_targets = vec![target.clone()];
    if let emusync_domain::library::GameEntry::Native(native) = &mut game.entry {
        native.common.save_targets = vec![target];
    }
    game
}

fn plan(tracked: &MockTrackedSync, path: &str, decision: Decision) {
    tracked
        .planned_decisions
        .lock()
        .unwrap()
        .insert(path.into(), decision);
}

fn connected() -> MockAuth {
    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("synthetic-token".into());
    auth
}

#[tokio::test]
async fn post_game_upload_skips_files_inside_an_excluded_folder() {
    let files = MockFs::default();
    for path in [KEPT, REPLAY, LOOKALIKE] {
        files.add_file(path, 1000);
    }
    let drive = MockDrive::default();
    drive.add_folder(&["CTR".into()], "ctr");
    let tracked = MockTrackedSync::default();
    for path in [KEPT, REPLAY, LOOKALIKE] {
        plan(&tracked, path, Decision::UploadLocal);
    }
    let sync = create_test_orchestrator(
        files,
        drive,
        tracked.clone(),
        connected(),
        MockLauncher::default(),
        sample_app_config(),
        |_| {},
    );

    let result = sync.play_with_sync(ctr_game(), &MockSetup::default()).await;

    assert_eq!(result, Ok(()));
    let mut uploads = tracked.uploads.lock().unwrap().clone();
    uploads.sort();
    assert_eq!(uploads, vec![LOOKALIKE.to_string(), KEPT.to_string()]);
}

#[tokio::test]
async fn pre_game_restore_leaves_excluded_cloud_files_in_the_cloud() {
    let files = MockFs::default();
    files.add_dir(ROOT);
    let drive = MockDrive::default();
    drive.add_folder(&["CTR".into()], "ctr");
    drive.add_file(
        "ctr",
        sample_remote_file("kept", "Savedata/1/CUSA/savedata0/SYS-DATA.sav", 5),
    );
    drive.add_file(
        "ctr",
        sample_remote_file("replay", "Savedata/1/CUSA/RaceReplay/race1.rep", 5),
    );
    let tracked = MockTrackedSync::default();
    plan(&tracked, KEPT, Decision::DownloadCloud);
    plan(&tracked, REPLAY, Decision::DownloadCloud);
    let sync = create_test_orchestrator(
        files,
        drive,
        tracked.clone(),
        connected(),
        MockLauncher::default(),
        sample_app_config(),
        |_| {},
    );

    let result = sync.play_with_sync(ctr_game(), &MockSetup::default()).await;

    assert_eq!(result, Ok(()));
    assert_eq!(*tracked.downloads.lock().unwrap(), vec![KEPT.to_string()]);
}

#[tokio::test]
async fn status_ignores_local_and_cloud_files_inside_an_excluded_folder() {
    let files = MockFs::default();
    files.add_file(KEPT, 1000);
    files.add_file(REPLAY, 1000);
    let drive = MockDrive::default();
    drive.add_folder(&["CTR".into()], "ctr");
    drive.add_file(
        "ctr",
        sample_remote_file("kept", "Savedata/1/CUSA/savedata0/SYS-DATA.sav", 5),
    );
    drive.add_file(
        "ctr",
        sample_remote_file("old", "Savedata/1/CUSA/RaceReplay/race0.rep", 5),
    );
    let tracked = MockTrackedSync::default();
    plan(
        &tracked,
        "/saves/ctr/1/CUSA/RaceReplay/race0.rep",
        Decision::DownloadCloud,
    );
    let sync = create_test_orchestrator(
        files,
        drive,
        tracked,
        connected(),
        MockLauncher::default(),
        sample_app_config(),
        |_| {},
    );

    // The replay exists only locally and another only in the cloud: neither counts.
    let status = sync.check_entry(&ctr_game().entry).await;

    assert_eq!(status, Ok(EntrySyncStatus::InSync));
}
