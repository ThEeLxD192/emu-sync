mod common;

use common::wait_for_active_tasks;
use emusync_data_local::ConfigManager;
use emusync_desktop_app::{
    AppEvent, DesktopCoordinator, apply_steam_state_refreshed, scan_entry_games,
};
use emusync_domain::{
    config::{AppConfig, ConfigurationStore},
    execution::{PathPicker, PickerType},
    library::{Emulator, EntryCommon, GameEntry, NativeGame},
};
use emusync_platform_desktop::{DesktopSteamLibrary, SteamShortcutManager};
use emusync_presentation::{
    actions::UiAction,
    model::{GameItem, PathTarget, PickMode},
};
use std::collections::HashSet;
use std::sync::{
    Arc,
    mpsc::{TryRecvError, channel},
};
use std::time::Duration;
use tempfile::tempdir;

fn test_native(name: &str) -> GameEntry {
    GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: name.to_string(),
            executable_path: "/bin/true".to_string(),
            arguments: vec![],
            save_paths: vec!["/tmp/saves".to_string()],
            save_targets: vec![],
            group: None,
            cloud_folder: None,
            drive_file_id: None,
            steam_app_id: None,
        },
        cover_path: None,
        wait_for_process: None,
    })
}

#[tokio::test]
async fn deleting_last_category_clears_library_cleanly() {
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let entry = test_native("OnlyCategory");
    manager
        .update(|_| AppConfig {
            credentials: None,
            entries: vec![entry.clone()],
        })
        .unwrap();

    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager.clone(),
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    );

    coordinator.handle_action(UiAction::DeleteEntry(entry));

    let mut saw_config_reloaded = false;
    while let Ok(event) = rx.try_recv() {
        if let AppEvent::ConfigReloaded(cfg) = event {
            assert!(cfg.entries.is_empty(), "Config should have 0 entries");
            saw_config_reloaded = true;
        }
    }
    assert!(saw_config_reloaded);
}

#[test]
fn stale_scan_event_with_lower_revision_is_ignored() {
    use emusync_desktop_app::apply_entry_games_scanned;

    let mut current_revision = 0u64;
    let mut displayed_games = vec![];

    // Event 1 with revision 2 arrives first for "Cat"
    let event1_games = vec![GameItem::new("GameV2", test_native("Cat"), None)];
    let applied1 = apply_entry_games_scanned(
        &mut current_revision,
        &mut displayed_games,
        Some("Cat"),
        "Cat",
        event1_games,
        2,
    );
    assert!(applied1);
    assert_eq!(current_revision, 2);
    assert_eq!(displayed_games[0].name, "GameV2");

    // Stale Event with revision 1 arrives later for "Cat"
    let event0_games = vec![GameItem::new("GameV1_Stale", test_native("Cat"), None)];
    let applied0 = apply_entry_games_scanned(
        &mut current_revision,
        &mut displayed_games,
        Some("Cat"),
        "Cat",
        event0_games,
        1,
    );
    assert!(!applied0);
    assert_eq!(current_revision, 2);
    assert_eq!(displayed_games[0].name, "GameV2");

    // Event for a different category arrives
    let other_games = vec![GameItem::new("OtherGame", test_native("OtherCat"), None)];
    let applied_other = apply_entry_games_scanned(
        &mut current_revision,
        &mut displayed_games,
        Some("Cat"),
        "OtherCat",
        other_games,
        3,
    );
    assert!(!applied_other);
    assert_eq!(current_revision, 2);
    assert_eq!(displayed_games[0].name, "GameV2");
}

#[tokio::test]
async fn scan_entry_games_handles_missing_roms_dir_and_filters_extensions_with_monotonic_revisions()
{
    let dir = tempdir().unwrap();
    let roms_dir = dir.path().join("roms");
    std::fs::create_dir(&roms_dir).unwrap();
    std::fs::write(roms_dir.join("Mario.sfc"), b"mario").unwrap();
    std::fs::write(roms_dir.join("Zelda.smc"), b"zelda").unwrap();
    std::fs::write(roms_dir.join("Readme.txt"), b"readme").unwrap();

    let missing_emu = Emulator {
        common: EntryCommon {
            name: "MissingEmu".into(),
            ..Default::default()
        },
        roms_directory: "/nonexistent/roms/dir".into(),
        extensions: vec!["sfc".into(), "smc".into()],
        ..Default::default()
    };
    let entry_missing = GameEntry::Emulator(missing_emu);

    let valid_emu = Emulator {
        common: EntryCommon {
            name: "ValidEmu".into(),
            ..Default::default()
        },
        roms_directory: roms_dir.to_str().unwrap().to_string(),
        extensions: vec!["sfc".into(), "smc".into()],
        ..Default::default()
    };
    let entry_valid = GameEntry::Emulator(valid_emu);

    assert!(scan_entry_games(&entry_missing).is_empty());
    let scanned_games = scan_entry_games(&entry_valid);
    assert_eq!(scanned_games.len(), 2);
    assert!(scanned_games.iter().any(|g| g.name == "Mario"));
    assert!(scanned_games.iter().any(|g| g.name == "Zelda"));
    assert!(!scanned_games.iter().any(|g| g.name == "Readme"));

    let (tx, rx) = std::sync::mpsc::channel();
    let config_path = dir.path().join("config.json");
    let manager = ConfigManager::new(&config_path).unwrap();
    let coordinator = DesktopCoordinator::new(
        manager,
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    );

    let (rev1, h1) = coordinator.platform.scan_entry_games_async(entry_missing);
    let (rev2, h2) = coordinator.platform.scan_entry_games_async(entry_valid);

    let _ = h1.await;
    let _ = h2.await;

    assert_eq!(rev1, 1);
    assert_eq!(rev2, 2);

    let events: Vec<_> = rx.try_iter().collect();
    let ev1 = events
        .iter()
        .find(|e| matches!(e, AppEvent::EntryGamesScanned { revision: 1, .. }))
        .unwrap();
    let ev2 = events
        .iter()
        .find(|e| matches!(e, AppEvent::EntryGamesScanned { revision: 2, .. }))
        .unwrap();

    match ev1 {
        AppEvent::EntryGamesScanned { games, .. } => assert!(games.is_empty()),
        _ => unreachable!(),
    }
    match ev2 {
        AppEvent::EntryGamesScanned { games, .. } => {
            assert_eq!(games.len(), 2);
            assert!(games.iter().any(|g| g.name == "Mario"));
            assert!(games.iter().any(|g| g.name == "Zelda"));
        }
        _ => unreachable!(),
    }
}

async fn refreshed_steam_state(
    steam: DesktopSteamLibrary,
    entries: Vec<GameEntry>,
) -> (bool, HashSet<String>) {
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let (tx, rx) = channel();
    let mut coordinator = DesktopCoordinator::new(
        manager,
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    );
    coordinator.platform.steam = steam;

    coordinator
        .platform
        .refresh_steam_state_async(entries)
        .await
        .unwrap();

    rx.try_iter()
        .find_map(|event| match event {
            AppEvent::SteamStateRefreshed {
                available,
                registered,
                ..
            } => Some((available, registered)),
            _ => None,
        })
        .expect("refresh must report the Steam state")
}

#[tokio::test]
async fn steam_refresh_reports_unavailable_without_steam_userdata() {
    let dir = tempdir().unwrap();
    let steam = DesktopSteamLibrary::new(
        SteamShortcutManager::new()
            .with_custom_userdata_dir(dir.path().join("missing"))
            .with_simulate_windows(false),
    );

    let (available, registered) =
        refreshed_steam_state(steam, vec![test_native("Hollow Knight")]).await;

    assert!(!available);
    assert!(registered.is_empty());
}

#[tokio::test]
async fn steam_refresh_reports_available_and_only_registered_entries() {
    let dir = tempdir().unwrap();
    let manager = SteamShortcutManager::new()
        .with_custom_shortcuts_file(dir.path().join("config/shortcuts.vdf"))
        .with_custom_launcher_dir(dir.path().join("launchers"))
        .with_simulate_windows(false);
    let registered_entry = test_native("Hollow Knight");
    manager.register_entry(&registered_entry).unwrap();
    let steam = DesktopSteamLibrary::new(manager);

    let (available, registered) =
        refreshed_steam_state(steam, vec![registered_entry, test_native("Celeste")]).await;

    assert!(available);
    assert_eq!(registered, HashSet::from(["Hollow Knight".to_string()]));
}

#[test]
fn stale_steam_refresh_does_not_overwrite_newer_state() {
    let mut revision = 0;
    let mut available = false;
    let mut registered = HashSet::new();

    assert!(apply_steam_state_refreshed(
        &mut revision,
        &mut available,
        &mut registered,
        true,
        HashSet::from(["New".to_string()]),
        2,
    ));
    assert!(!apply_steam_state_refreshed(
        &mut revision,
        &mut available,
        &mut registered,
        false,
        HashSet::new(),
        1,
    ));

    assert!(available);
    assert_eq!(registered, HashSet::from(["New".to_string()]));
    assert_eq!(revision, 2);
}

/// Picker that answers every request with a scripted result and reports each call.
struct ScriptedPicker {
    answer: fn() -> std::io::Result<Option<String>>,
}

impl PathPicker for ScriptedPicker {
    type Error = std::io::Error;

    fn pick(
        &self,
        _label: &str,
        _initial_path: &str,
        _kind: PickerType,
    ) -> Result<Option<String>, Self::Error> {
        (self.answer)()
    }
}

fn coordinator_with_picker(
    answer: fn() -> std::io::Result<Option<String>>,
) -> (DesktopCoordinator, std::sync::mpsc::Receiver<AppEvent>) {
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager,
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    )
    .with_path_picker(Arc::new(ScriptedPicker { answer }));
    (coordinator, rx)
}

fn pick_cover_file(request_id: u64) -> UiAction {
    UiAction::PickPath {
        request_id,
        target: PathTarget::CoverPath,
        mode: PickMode::File,
    }
}

#[tokio::test]
async fn picked_path_event_echoes_the_request_id() {
    let (coordinator, rx) = coordinator_with_picker(|| Ok(Some("/x".to_string())));

    coordinator.handle_action(pick_cover_file(42));

    let event = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("path picked");
    assert_eq!(
        event,
        AppEvent::PathPicked {
            request_id: 42,
            target: PathTarget::CoverPath,
            mode: PickMode::File,
            path: "/x".to_string(),
        }
    );
}

#[tokio::test]
async fn cancelled_picker_sends_no_event() {
    let (coordinator, rx) = coordinator_with_picker(|| Ok(None));

    coordinator.handle_action(pick_cover_file(7));
    wait_for_active_tasks(&coordinator).await;

    assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn failing_picker_reports_a_toast() {
    let (coordinator, rx) = coordinator_with_picker(|| Err(std::io::Error::other("no portal")));

    coordinator.handle_action(pick_cover_file(7));
    wait_for_active_tasks(&coordinator).await;

    match rx.try_recv() {
        Ok(AppEvent::Toast(message)) => {
            assert!(
                message.starts_with("Could not open file chooser:"),
                "{message}"
            );
        }
        other => panic!("expected a toast, got {other:?}"),
    }
    assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn restore_preview_event_echoes_the_request_id() {
    let config_dir = tempdir().unwrap();
    let destination_dir = tempdir().unwrap();
    let manager = ConfigManager::new(&config_dir.path().join("config.json")).unwrap();
    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager,
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    );
    let destination = destination_dir.path().to_string_lossy().into_owned();

    coordinator.handle_action(UiAction::PreviewRestorePaths {
        request_id: 7,
        destination: destination.clone(),
        names: vec!["a.sav".to_string()],
    });

    let event = tokio::task::spawn_blocking(move || {
        loop {
            match rx.recv_timeout(std::time::Duration::from_secs(10)) {
                Ok(event @ AppEvent::RestorePathsPreviewed { .. }) => return event,
                Ok(_) => {}
                Err(err) => panic!("no RestorePathsPreviewed event: {err}"),
            }
        }
    })
    .await
    .unwrap();
    let AppEvent::RestorePathsPreviewed {
        request_id,
        destination: echoed,
        paths,
    } = event
    else {
        unreachable!()
    };
    assert_eq!(request_id, 7);
    assert_eq!(echoed, destination);
    assert_eq!(
        paths,
        vec![
            destination_dir
                .path()
                .join("a.sav")
                .to_string_lossy()
                .into_owned()
        ]
    );
}
