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
    assert!(app.take_actions().is_empty());
}

#[test]
fn app_opens_and_submits_add_category_modal() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();

    app.open_add_entry();
    assert!(matches!(app.active_dialog(), ActiveDialog::Entry(_)));

    // Fill the active form
    if let Some(form) = app.entry_form_mut() {
        form.name = "GameBoy Advance".to_string();
        form.executable_path = "mgba".to_string();
        form.roms_directory = "/roms/gba".to_string();
        form.extensions = "gba".to_string();
        form.saves.update_path(0, "/saves/gba".to_string());
    }

    click_button(&ctx, &mut app, "Add");

    let actions = app.take_actions();
    assert_eq!(actions.len(), 1);
    match &actions[0] {
        UiAction::AddEntry(entry) => {
            assert_eq!(entry.common().name, "GameBoy Advance");
            assert_eq!(entry.common().executable_path, "mgba");
        }
        _ => panic!("expected AddEntry action"),
    }
    assert_eq!(app.active_dialog(), &ActiveDialog::None);
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
    assert_eq!(app.active_dialog(), &ActiveDialog::Update);

    let ctx = Context::default();
    click_button(&ctx, &mut app, "Update Now");
    assert_eq!(app.take_actions(), vec![UiAction::DownloadUpdate(info)]);
}

fn cancel() -> UiAction {
    UiAction::CancelSaveSetup { request_id: 1 }
}

fn setup_request() -> SaveSetupRequest {
    let entry = GameEntry::Emulator(Emulator {
        common: EntryCommon {
            name: "SNES".to_string(),
            ..EntryCommon::default()
        },
        ..Emulator::default()
    });
    SaveSetupRequest {
        id: 1,
        item: GameItem::new("smw.sfc", entry, None),
        notice: Some("Configure saves before play".to_string()),
        cloud_file_names: Vec::new(),
        targets: Vec::new(),
    }
}

fn text_center(ctx: &Context, app: &mut EmuSyncApp, label: &str) -> Option<egui::Pos2> {
    let input = RawInput {
        screen_rect: Some(screen()),
        ..RawInput::default()
    };
    let output = ctx.run_ui(input, |ui| app.update(ui));
    let center = output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(text.visual_bounding_rect().center())
            }
            _ => None,
        });
    output.drop_without_applying_deltas();
    center
}

/// Center of the text `label` once the dialog layout has settled (it moves over its
/// first frames while it sizes itself).
fn center_of_text(ctx: &Context, app: &mut EmuSyncApp, label: &str) -> egui::Pos2 {
    let mut previous = None;
    for _ in 0..12 {
        let current = text_center(ctx, app, label);
        if current.is_some() && current == previous {
            return current.unwrap_or_default();
        }
        previous = current;
    }
    panic!("`{label}` never settled");
}

fn click_button(ctx: &Context, app: &mut EmuSyncApp, label: &str) {
    let pos = center_of_text(ctx, app, label);
    run_frames(ctx, app, click_at(pos));
}

#[test]
fn save_setup_save_completes_the_same_request() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    let request = setup_request();
    app.open_save_setup(&request);

    click_button(&ctx, &mut app, "Save");

    let actions = app.take_actions();
    assert!(
        matches!(
            actions.as_slice(),
            [UiAction::CompleteSaveSetup { request: sent, .. }] if **sent == request
        ),
        "{actions:?}"
    );
    assert!(matches!(app.active_dialog(), ActiveDialog::None));
}

#[test]
fn save_setup_cancel_and_escape_each_emit_one_cancel() {
    for dismiss in ["Cancel", "Escape"] {
        let ctx = Context::default();
        let mut app = EmuSyncApp::new();
        app.open_save_setup(&setup_request());
        run_frames(&ctx, &mut app, vec![Vec::new()]);

        if dismiss == "Cancel" {
            click_button(&ctx, &mut app, "Cancel");
        } else {
            run_frames(&ctx, &mut app, vec![escape()]);
        }

        assert_eq!(app.take_actions(), vec![cancel()], "{dismiss}");
        assert!(matches!(app.active_dialog(), ActiveDialog::None));
    }
}

#[test]
fn replacing_a_save_setup_cancels_the_previous_request() {
    let mut app = EmuSyncApp::new();
    app.open_save_setup(&setup_request());
    app.open_save_setup(&setup_request());
    assert_eq!(app.take_actions(), vec![cancel()]);

    app.close_dialog();
    assert_eq!(app.take_actions(), vec![cancel()]);

    app.close_dialog();
    assert!(app.take_actions().is_empty());
}

#[test]
fn close_save_setup_closes_the_matching_request_without_cancelling() {
    let mut app = EmuSyncApp::new();
    app.open_save_setup(&setup_request());

    app.close_save_setup(1);

    assert!(matches!(app.active_dialog(), ActiveDialog::None));
    assert!(app.take_actions().is_empty());
}

#[test]
fn close_save_setup_for_another_id_keeps_the_dialog() {
    let mut app = EmuSyncApp::new();
    app.open_save_setup(&setup_request());

    app.close_save_setup(2);

    assert!(matches!(app.active_dialog(), ActiveDialog::GameEdit { .. }));
    assert!(app.take_actions().is_empty());
}

#[test]
fn close_save_setup_leaves_other_dialogs_untouched() {
    let mut app = EmuSyncApp::new();
    app.open_add_entry();
    app.close_save_setup(1);
    assert!(matches!(app.active_dialog(), ActiveDialog::Entry(_)));

    app.close_dialog();
    app.state.update_state = emusync_application::UpdateState::Available(UpdateInfo {
        version: "1.3.0".to_string(),
        tag_name: "v1.3.0".to_string(),
        release_notes: String::new(),
        download_url: "https://example.com/app.AppImage".to_string(),
        asset_size: 1,
        asset_name: "EmuSync.AppImage".to_string(),
    });
    app.open_update_dialog();
    app.close_save_setup(1);
    assert_eq!(app.active_dialog(), &ActiveDialog::Update);
}

#[test]
fn game_override_save_emits_override_without_cancel() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    let item = setup_request().item;
    app.open_edit_game(&item, &[]);

    click_button(&ctx, &mut app, "Save");

    let actions = app.take_actions();
    assert!(
        matches!(actions.as_slice(), [UiAction::EditGameOverride { .. }]),
        "{actions:?}"
    );
}

#[test]
fn save_setup_stays_clickable_above_a_launch_status() {
    for status in [AppStatus::Playing, AppStatus::Loading] {
        let ctx = Context::default();
        let mut app = EmuSyncApp::new();
        app.state.status = status.clone();
        app.open_save_setup(&setup_request());

        click_button(&ctx, &mut app, "Save");

        let actions = app.take_actions();
        assert!(
            matches!(actions.as_slice(), [UiAction::CompleteSaveSetup { .. }]),
            "{status:?}: {actions:?}"
        );
    }
}

#[test]
fn drive_secret_never_appears_in_debug_output() {
    use emusync_presentation::dialogs::DriveSetupDialogState;

    let action = UiAction::ConnectDrive {
        client_id: "id".into(),
        client_secret: "GOCSPX-secret".into(),
    };
    assert!(!format!("{action:?}").contains("GOCSPX-secret"));
    let state = DriveSetupDialogState::new("id".into(), "GOCSPX-secret".into(), false);
    assert!(!format!("{state:?}").contains("GOCSPX-secret"));
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
    assert!(matches!(
        app.active_dialog(),
        ActiveDialog::ConfirmDelete(_)
    ));
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

    assert!(matches!(app.active_dialog(), ActiveDialog::None));
    assert!(
        !app.take_actions()
            .iter()
            .any(|a| matches!(a, UiAction::DeleteEntry(_)))
    );
}

#[test]
fn picked_save_path_takes_the_picked_kind_and_stale_detection_is_ignored() {
    use emusync_domain::library::SaveTargetType;
    use emusync_presentation::model::{PathTarget, PickMode};

    let mut app = EmuSyncApp::new();
    app.open_add_entry();

    app.apply_picked_path(
        PathTarget::SavePath(0),
        PickMode::Directory,
        "/saves/folder".to_string(),
    );
    let ActiveDialog::Entry(form) = &app.active_dialog() else {
        panic!("entry dialog must stay open");
    };
    assert_eq!(form.saves.rows[0].path, "/saves/folder");
    assert_eq!(form.saves.rows[0].target_type, SaveTargetType::Directory);

    // A detection for a path the row no longer holds must not change its kind.
    app.apply_detected_save_type(0, "/saves/old-file", SaveTargetType::File);
    let ActiveDialog::Entry(form) = &app.active_dialog() else {
        unreachable!()
    };
    assert_eq!(form.saves.rows[0].target_type, SaveTargetType::Directory);

    app.apply_detected_save_type(0, "/saves/folder", SaveTargetType::File);
    let ActiveDialog::Entry(form) = &app.active_dialog() else {
        unreachable!()
    };
    assert_eq!(form.saves.rows[0].target_type, SaveTargetType::File);

    app.apply_picked_path(
        PathTarget::Executable,
        PickMode::File,
        "/usr/bin/mgba".to_string(),
    );
    let ActiveDialog::Entry(form) = &app.active_dialog() else {
        unreachable!()
    };
    assert_eq!(form.executable_path, "/usr/bin/mgba");
}

#[test]
fn entry_dialog_renders_both_entry_types_headless() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.open_add_entry();
    for _ in 0..2 {
        let output = ctx.run_ui(RawInput::default(), |ui| app.update(ui));
        output.drop_without_applying_deltas();
        if let Some(form) = app.entry_form_mut() {
            form.entry_type = emusync_presentation::EntryType::Native;
        }
    }
    assert!(matches!(app.active_dialog(), ActiveDialog::Entry(_)));
    assert!(app.take_actions().is_empty());
}

fn screen() -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))
}

fn escape() -> Vec<egui::Event> {
    vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]
}

/// Frames that move the pointer to `pos`, press and release the primary button.
fn click_at(pos: egui::Pos2) -> Vec<Vec<egui::Event>> {
    let button = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    vec![
        vec![egui::Event::PointerMoved(pos)],
        vec![button(true)],
        vec![button(false)],
        Vec::new(),
    ]
}

fn run_frames(ctx: &Context, app: &mut EmuSyncApp, frames: Vec<Vec<egui::Event>>) {
    for events in frames {
        let input = RawInput {
            screen_rect: Some(screen()),
            events,
            ..RawInput::default()
        };
        let output = ctx.run_ui(input, |ui| app.update(ui));
        output.drop_without_applying_deltas();
    }
}

#[test]
fn operation_overlays_ignore_escape_and_clicks_outside() {
    let statuses = [
        AppStatus::Playing,
        AppStatus::Loading,
        AppStatus::Syncing {
            message: "Downloading saves...".to_string(),
        },
        AppStatus::Conflict {
            local_date: "today".to_string(),
            cloud_date: "yesterday".to_string(),
            game_name: "Celeste".to_string(),
        },
        AppStatus::PostSyncFailed {
            game_name: "Celeste".to_string(),
            reason: "Network unavailable".to_string(),
            completed_save_count: 0,
            pending_save_count: 1,
        },
    ];
    for status in statuses {
        let ctx = Context::default();
        let mut app = EmuSyncApp::new();
        app.state.status = status.clone();

        let mut frames = vec![escape()];
        frames.extend(click_at(egui::pos2(5.0, 5.0)));
        run_frames(&ctx, &mut app, frames);

        assert_eq!(app.state.status, status);
        assert!(
            app.take_actions().is_empty(),
            "{status:?} must not be dismissed by Escape or a click outside"
        );
    }
}

#[test]
fn conflict_accepts_one_choice_and_then_shows_syncing() {
    use emusync_presentation::dialogs::conflict_card_id;

    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.state.status = AppStatus::Conflict {
        local_date: "today".to_string(),
        cloud_date: "yesterday".to_string(),
        game_name: "Celeste".to_string(),
    };
    run_frames(&ctx, &mut app, vec![Vec::new()]);
    let local = ctx
        .read_response(conflict_card_id("Local Save"))
        .expect("local card is shown")
        .rect
        .center();
    let cloud = ctx
        .read_response(conflict_card_id("Cloud Save"))
        .expect("cloud card is shown")
        .rect
        .center();

    run_frames(&ctx, &mut app, click_at(local));
    assert_eq!(
        app.take_actions(),
        vec![UiAction::ResolveConflict(Decision::UploadLocal)]
    );

    // While the coordinator has not changed the status, the other option is gone.
    run_frames(&ctx, &mut app, click_at(cloud));
    assert!(
        app.take_actions().is_empty(),
        "a second choice must not be sent"
    );
    assert!(ctx.read_response(conflict_card_id("Cloud Save")).is_none());

    // A new conflict is asked again.
    app.state.status = AppStatus::Conflict {
        local_date: "now".to_string(),
        cloud_date: "today".to_string(),
        game_name: "Hollow Knight".to_string(),
    };
    run_frames(&ctx, &mut app, vec![Vec::new(), Vec::new()]);
    assert!(ctx.read_response(conflict_card_id("Cloud Save")).is_some());
}

#[test]
fn error_notice_closes_on_escape_like_kotlin() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.state.status = AppStatus::Error {
        message: "Drive quota exceeded".to_string(),
    };

    run_frames(&ctx, &mut app, vec![Vec::new(), escape()]);

    assert_eq!(app.state.status, AppStatus::Idle);
    assert_eq!(app.take_actions(), vec![UiAction::ClearStatus]);
}

#[test]
fn post_sync_failure_message_reports_partial_progress() {
    use emusync_presentation::dialogs::post_sync_failed_message;

    assert_eq!(
        post_sync_failed_message("Celeste", "Network unavailable", 0, 2),
        "Celeste closed normally. Google Drive was not updated.\n\nNetwork unavailable"
    );
    assert_eq!(
        post_sync_failed_message("Celeste", "Quota", 1, 2),
        "Celeste closed normally. 1 save(s) synced successfully; 2 save(s) still need to be uploaded.\n\nQuota"
    );
}

#[test]
fn drive_setup_sends_trimmed_credentials_and_flags_blank_ones() {
    use emusync_presentation::dialogs::DriveSetupDialogState;

    let mut state = DriveSetupDialogState::new("  id.apps  ".into(), "   ".into(), false);
    assert!(!state.secret_revealed, "the secret starts masked");

    assert_eq!(state.submit(), None, "a blank secret must not connect");
    assert_eq!(
        state.error_message.as_deref(),
        Some("Please enter both Client ID and Client Secret.")
    );

    state.field_edited();
    assert_eq!(state.error_message, None, "editing clears the error");

    state.client_secret = " GOCSPX-secret ".into();
    assert_eq!(
        state.submit(),
        Some(("id.apps".to_string(), "GOCSPX-secret".to_string()))
    );
}

#[test]
fn drive_setup_opens_with_stored_credentials_and_closes_on_escape() {
    use emusync_domain::config::Credentials;

    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.state.config_path = "/home/user/.config/emusync/config.json".into();
    app.state.config = Some(AppConfig {
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: Vec::new(),
    });

    app.open_drive_setup();
    let ActiveDialog::DriveSetup(state) = &app.active_dialog() else {
        panic!("drive dialog must open");
    };
    assert_eq!(
        (state.client_id.as_str(), state.client_secret.as_str()),
        ("id", "secret")
    );
    assert!(
        state.is_linked,
        "a stored refresh token shows the unlink action"
    );

    run_frames(&ctx, &mut app, vec![Vec::new(), escape()]);

    assert!(matches!(app.active_dialog(), ActiveDialog::None));
    assert!(
        app.take_actions().is_empty(),
        "closing must not connect or unlink"
    );
}

#[test]
fn update_dialog_cannot_be_dismissed_while_downloading() {
    use emusync_application::UpdateState;

    let info = UpdateInfo {
        version: "9.9.9".into(),
        tag_name: "v9.9.9".into(),
        release_notes: "Notes".into(),
        download_url: String::new(),
        asset_size: 0,
        asset_name: "asset".into(),
    };
    for (state, closes) in [
        (
            UpdateState::Downloading {
                progress: 0.5,
                info: info.clone(),
            },
            false,
        ),
        (UpdateState::Available(info), true),
    ] {
        let ctx = Context::default();
        let mut app = EmuSyncApp::new();
        app.state.update_state = state.clone();
        app.open_update_dialog();

        run_frames(&ctx, &mut app, vec![Vec::new(), escape()]);

        assert_eq!(
            matches!(app.active_dialog(), ActiveDialog::None),
            closes,
            "Escape with {state:?}"
        );
        assert!(app.take_actions().is_empty());
    }
}
