//! Carpetas antiguas que comparten la ubicación en la nube: en un dispositivo nuevo los
//! archivos de la nube se restauran en una sola carpeta (deducida o elegida) en lugar de
//! detener la sincronización, y nada local se sobrescribe ni se duplica.

mod common;

use common::*;
use emusync_domain::library::{EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType};
use emusync_domain::sync::SavePlacementRequest;
use std::sync::Arc;

const SAVEDATA: &str = "/sd/Saves/savedata/1/CUSA13795";
const TROPHIES: &str = "/sd/Saves/trophies/CUSA13795";

fn legacy_targets() -> Vec<SaveTarget> {
    [SAVEDATA, TROPHIES]
        .into_iter()
        .map(|path| SaveTarget {
            path: path.into(),
            kind: SaveTargetType::Directory,
            cloud_path: None,
        })
        .collect()
}

fn game_entry() -> GameEntry {
    GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "CTR".into(),
            executable_path: "/games/ctr".into(),
            save_targets: legacy_targets(),
            ..EntryCommon::default()
        },
        wait_for_process: None,
        cover_path: None,
    })
}

/// A device whose folders exist but are empty, with the other device's saves in Drive.
fn new_device() -> (MockFs, MockDrive, MockTrackedSync, MockAuth) {
    let fs = MockFs::new();
    fs.add_dir(SAVEDATA);
    fs.add_dir(TROPHIES);
    let drive = MockDrive::default();
    drive.add_folder(&["CTR".into()], "f_ctr");
    for name in [
        "savedata1/SYS-DATA.sav",
        "savedata1.sce_sys/param.bin",
        "savedata1.sce_sys/icon0.png",
    ] {
        drive.add_file(
            "f_ctr",
            sample_remote_file(&format!("id-{name}"), name, 1_000),
        );
    }
    let tracked = MockTrackedSync::default();
    *tracked.materialize_in.lock().unwrap() = Some(fs.clone());
    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("token".into());
    (fs, drive, tracked, auth)
}

type Placements = tokio::sync::mpsc::UnboundedReceiver<SavePlacementRequest>;

fn orchestrator(
    fs: MockFs,
    drive: MockDrive,
    tracked: MockTrackedSync,
    auth: MockAuth,
    launcher: MockLauncher,
) -> (
    Arc<impl SaveSynchronizer<Error = String> + Send + Sync + 'static>,
    Placements,
    Arc<std::sync::Mutex<Vec<SyncEvent>>>,
) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = Arc::clone(&events);
    let orchestrator = create_test_orchestrator(
        fs,
        drive,
        tracked,
        auth,
        launcher,
        sample_app_config(),
        move |event| {
            if let SyncEvent::ChooseSaveFolders(request) = &event {
                let _ = tx.send(request.clone());
            }
            recorded.lock().unwrap().push(event);
        },
    );
    (Arc::new(orchestrator), rx, events)
}

fn sorted(mut paths: Vec<String>) -> Vec<String> {
    paths.sort();
    paths
}

#[tokio::test]
async fn chosen_folders_receive_each_cloud_file_once_and_the_sync_completes() {
    let (fs, drive, tracked, auth) = new_device();
    let (sync, mut placements, _) =
        orchestrator(fs, drive, tracked.clone(), auth, MockLauncher::default());

    let task = {
        let sync = Arc::clone(&sync);
        tokio::spawn(async move { sync.sync_entry(&game_entry()).await })
    };
    let request = placements.recv().await.expect("the user is asked");
    assert_eq!(request.game_name, "CTR");
    assert_eq!(
        request.folders,
        vec![SAVEDATA.to_string(), TROPHIES.to_string()]
    );
    let suggested: Vec<_> = request.groups.iter().map(|g| g.suggested_folder).collect();
    assert_eq!(suggested, vec![Some(0), Some(0)]);

    sync.resolve_save_placement(&request, Some(vec![0, 0]));

    assert_eq!(task.await.unwrap(), Ok(true));
    // Restored only into the chosen folder; the sibling does not get copies.
    assert_eq!(
        sorted(tracked.downloads.lock().unwrap().clone()),
        vec![
            format!("{SAVEDATA}/savedata1.sce_sys/icon0.png"),
            format!("{SAVEDATA}/savedata1.sce_sys/param.bin"),
            format!("{SAVEDATA}/savedata1/SYS-DATA.sav"),
        ]
    );
    assert!(tracked.uploads.lock().unwrap().is_empty());
}

#[tokio::test]
async fn cancelling_the_folder_choice_restores_nothing_and_reports_why() {
    let (fs, drive, tracked, auth) = new_device();
    let (sync, mut placements, _) =
        orchestrator(fs, drive, tracked.clone(), auth, MockLauncher::default());

    let task = {
        let sync = Arc::clone(&sync);
        tokio::spawn(async move { sync.sync_entry(&game_entry()).await })
    };
    let request = placements.recv().await.expect("the user is asked");
    // A stale or malformed answer is not taken as a choice.
    sync.resolve_save_placement(&request, Some(vec![0]));

    let result = task.await.unwrap();
    assert!(
        matches!(&result, Err(msg) if msg.contains("no folder was chosen")),
        "got {result:?}"
    );
    assert!(tracked.downloads.lock().unwrap().is_empty());
}

#[tokio::test]
async fn before_playing_a_folder_implied_by_the_local_layout_is_used_without_asking() {
    let (fs, drive, tracked, auth) = new_device();
    // The game already created its save directory on this device, but not the files.
    fs.add_dir(&format!("{SAVEDATA}/savedata1"));
    fs.add_dir(&format!("{SAVEDATA}/savedata1.sce_sys"));
    let launcher = MockLauncher::default();
    let (sync, mut placements, events) =
        orchestrator(fs, drive, tracked.clone(), auth, launcher.clone());
    let game = SyncGame {
        name: "CTR".into(),
        entry: game_entry(),
        rom_path: None,
        effective_title: "CTR".into(),
        save_targets: legacy_targets(),
    };

    let result = sync.play_with_sync(game, &MockSetup::default()).await;

    assert_eq!(result, Ok(()), "events: {:?}", events.lock().unwrap());
    assert!(placements.try_recv().is_err(), "nothing was ambiguous");
    assert_eq!(launcher.launched_games.lock().unwrap().len(), 1);
    assert_eq!(
        sorted(tracked.downloads.lock().unwrap().clone()),
        vec![
            format!("{SAVEDATA}/savedata1.sce_sys/icon0.png"),
            format!("{SAVEDATA}/savedata1.sce_sys/param.bin"),
            format!("{SAVEDATA}/savedata1/SYS-DATA.sav"),
        ]
    );
}
