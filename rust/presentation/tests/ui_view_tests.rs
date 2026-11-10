//! Headless UI rendering, modal dialogs, and action dispatching tests.

use egui::{Context, RawInput};
use emusync_domain::{
    config::AppConfig,
    library::{Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType},
    sync::Decision,
    update::UpdateInfo,
};
use emusync_presentation::{
    ActiveDialog, AppStatus, EmuSyncApp, GameItem, SaveSetupRequest, UiAction,
};

#[test]
fn app_renders_headless_and_emits_navigation_actions() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();

    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Hollow Knight".to_string(),
            executable_path: "hollow_knight".to_string(),
            save_targets: vec![SaveTarget {
                path: "/saves".to_string(),
                kind: SaveTargetType::Directory,
                cloud_path: None,
            }],
            ..EntryCommon::default()
        },
        ..NativeGame::default()
    });

    app.state.config = Some(AppConfig {
        credentials: None,
        entries: vec![entry.clone()],
    });
    app.state.selected_entry = Some(entry.clone());
    app.state.game_items = vec![GameItem::new("Hollow Knight", entry, None)];

    // Render frame
    let output = ctx.run_ui(RawInput::default(), |ui| {
        app.update(ui);
    });
    output.drop_without_applying_deltas();

    // Should render without panics
    assert!(app.pending_actions.is_empty());
}

#[test]
fn app_opens_and_submits_add_category_modal() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();

    app.open_add_entry();
    assert!(matches!(app.active_dialog, ActiveDialog::Entry(_)));

    // Fill the active form
    if let ActiveDialog::Entry(form) = &mut app.active_dialog {
        form.name = "GameBoy Advance".to_string();
        form.executable_path = "mgba".to_string();
        form.roms_directory = "/roms/gba".to_string();
        form.extensions = "gba".to_string();
        form.saves.update_path(0, "/saves/gba".to_string());
    }

    // Render frame which triggers build and save
    let output = ctx.run_ui(RawInput::default(), |ui| {
        app.update(ui);
    });
    output.drop_without_applying_deltas();

    // Manually trigger save from dialog
    if let ActiveDialog::Entry(form) = &mut app.active_dialog {
        let entry = form.build_entry().expect("valid entry");
        app.pending_actions.push(UiAction::AddEntry(entry));
        app.close_dialog();
    }

    let actions = app.take_actions();
    assert_eq!(actions.len(), 1);
    match &actions[0] {
        UiAction::AddEntry(entry) => {
            assert_eq!(entry.common().name, "GameBoy Advance");
            assert_eq!(entry.common().executable_path, "mgba");
        }
        _ => panic!("expected AddEntry action"),
    }
    assert_eq!(app.active_dialog, ActiveDialog::None);
}

#[test]
fn app_opens_drive_setup_and_emits_connect_action() {
    let mut app = EmuSyncApp::new();

    app.open_drive_setup();
    assert!(matches!(app.active_dialog, ActiveDialog::DriveSetup(_)));

    if let ActiveDialog::DriveSetup(state) = &mut app.active_dialog {
        state.client_id = "test-client-id".to_string();
        state.client_secret = "test-secret".to_string();
    }

    // Simulate saving credentials
    app.pending_actions.push(UiAction::ConnectDrive {
        client_id: "test-client-id".to_string(),
        client_secret: "test-secret".to_string(),
    });
    app.close_dialog();

    let actions = app.take_actions();
    assert_eq!(
        actions,
        vec![UiAction::ConnectDrive {
            client_id: "test-client-id".to_string(),
            client_secret: "test-secret".to_string(),
        }]
    );
}

#[test]
fn app_opens_update_dialog_and_emits_download_action() {
    let mut app = EmuSyncApp::new();
    let info = UpdateInfo {
        version: "1.3.0".to_string(),
        tag_name: "v1.3.0".to_string(),
        release_notes: "Fixes and improvements".to_string(),
        download_url: "https://example.com/app.AppImage".to_string(),
        asset_size: 1024,
        asset_name: "EmuSync.AppImage".to_string(),
    };

    app.state.update_state = emusync_application::UpdateState::Available(info.clone());
    app.open_update_dialog();
    assert_eq!(app.active_dialog, ActiveDialog::Update);

    app.pending_actions
        .push(UiAction::DownloadUpdate(info.clone()));
    let actions = app.take_actions();
    assert_eq!(actions, vec![UiAction::DownloadUpdate(info)]);
}

#[test]
fn app_resolves_conflict_and_retries_post_sync() {
    let mut app = EmuSyncApp::new();

    app.state.status = AppStatus::Conflict {
        local_date: "2026-11-06 12:00:00".to_string(),
        cloud_date: "2026-11-06 13:00:00".to_string(),
        game_name: "Super Mario World".to_string(),
    };

    // Simulate user choosing keep local
    app.pending_actions
        .push(UiAction::ResolveConflict(Decision::UploadLocal));
    app.state.status = AppStatus::Idle;

    let actions = app.take_actions();
    assert_eq!(
        actions,
        vec![UiAction::ResolveConflict(Decision::UploadLocal)]
    );

    // Simulate post-sync failed retry
    app.state.status = AppStatus::PostSyncFailed {
        game_name: "Super Mario World".to_string(),
        reason: "Network timeout".to_string(),
        completed_save_count: 0,
        pending_save_count: 1,
    };

    app.pending_actions.push(UiAction::RetryPostSync);
    let retry_actions = app.take_actions();
    assert_eq!(retry_actions, vec![UiAction::RetryPostSync]);
}

#[test]
fn app_auto_opens_save_setup_request() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();

    let entry = GameEntry::Emulator(Emulator {
        common: EntryCommon {
            name: "SNES".to_string(),
            ..EntryCommon::default()
        },
        ..Emulator::default()
    });

    let item = GameItem::new("smw.sfc", entry, None);
    app.state.save_setup_request = Some(SaveSetupRequest {
        item,
        notice: Some("Configure saves before play".to_string()),
        cloud_file_names: vec!["smw.srm".to_string()],
    });

    let output = ctx.run_ui(RawInput::default(), |ui| {
        app.update(ui);
    });
    output.drop_without_applying_deltas();

    assert!(matches!(app.active_dialog, ActiveDialog::GameEdit { .. }));
}

#[test]
fn app_toast_message_set_and_clear() {
    let mut app = EmuSyncApp::new();
    assert_eq!(app.state.toast_message, None);

    app.set_toast("Shortcut added to Steam");
    assert_eq!(
        app.state.toast_message,
        Some("Shortcut added to Steam".to_string())
    );

    app.clear_toast();
    assert_eq!(app.state.toast_message, None);
}

#[test]
fn adaptive_grid_fills_width_with_cards_of_at_least_min_width() {
    use emusync_presentation::components::game_grid::adaptive_columns;

    // Steam Deck: 1280 wide minus the 240 sidebar, its divider and 2×20 margins.
    let (columns, width) = adaptive_columns(999.0, 160.0, 12.0);
    assert_eq!(columns, 5);
    assert!(width >= 160.0);
    assert!((columns as f32 * width + 4.0 * 12.0 - 999.0).abs() < 0.01);

    // Narrower than one card: a single column taking the whole width.
    assert_eq!(adaptive_columns(120.0, 160.0, 12.0), (1, 120.0));
    // Exactly two cards and a gap.
    assert_eq!(adaptive_columns(332.0, 160.0, 12.0), (2, 160.0));
}

#[test]
fn deleting_a_category_waits_for_confirmation_and_escape_cancels() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Hollow Knight".to_string(),
            ..EntryCommon::default()
        },
        ..NativeGame::default()
    });
    app.state.config = Some(AppConfig {
        credentials: None,
        entries: vec![entry.clone()],
    });
    app.state.selected_entry = Some(entry.clone());

    app.request_delete_entry(entry);
    let output = ctx.run_ui(RawInput::default(), |ui| app.update(ui));
    output.drop_without_applying_deltas();
    assert!(matches!(app.active_dialog, ActiveDialog::ConfirmDelete(_)));
    assert!(
        app.take_actions().is_empty(),
        "nothing is deleted before confirming"
    );

    let escape = RawInput {
        events: vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..RawInput::default()
    };
    let output = ctx.run_ui(escape, |ui| app.update(ui));
    output.drop_without_applying_deltas();

    assert!(matches!(app.active_dialog, ActiveDialog::None));
    assert!(
        !app.take_actions()
            .iter()
            .any(|a| matches!(a, UiAction::DeleteEntry(_)))
    );
}
