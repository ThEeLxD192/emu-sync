//! Main window: header, sidebar and game grid interactions through headless frames.

mod common;

use common::*;
use egui::{Context, Pos2};
use emusync_application::UpdateState;
use emusync_domain::{config::Credentials, library::GameEntry};
use emusync_presentation::{
    ActiveDialog, AppStatus, AppUiState, CloudSyncStatus, EmuSyncApp, GameItem, UiAction,
    components::header::connect_drive_action,
    theme::glyph::{CHEVRON_DOWN, CHEVRON_UP, PENCIL, REFRESH, WIDGETS},
};

fn hollow_knight() -> GameEntry {
    native_entry("Hollow Knight")
}

fn gba() -> GameEntry {
    emulator_entry("GBA", "/roms", &["gba"])
}

fn hollow_knight_item() -> GameItem {
    GameItem::new("Hollow Knight", hollow_knight(), None)
}

/// A library of `entries` with the first one selected and its game on the grid.
fn app_with(entries: Vec<GameEntry>) -> EmuSyncApp {
    let mut app = EmuSyncApp::new();
    app.state.selected_entry = entries.first().cloned();
    app.state.game_items = vec![hollow_knight_item()];
    app.state.config = Some(config_with(entries, None));
    app
}

fn complete_credentials(token: Option<&str>, secret: &str) -> Credentials {
    Credentials {
        client_id: "id".to_string(),
        client_secret: secret.to_string(),
        refresh_token: token.map(str::to_string),
    }
}

fn settle(ctx: &Context, app: &mut EmuSyncApp) {
    run_frames(ctx, app, (0..8).map(|_| Vec::new()).collect());
}

/// The game card title: the lowest "Hollow Knight" text (the sidebar and toolbar
/// show the same name above it).
fn card_title(ctx: &Context, app: &mut EmuSyncApp) -> Pos2 {
    settle(ctx, app);
    all_text_centers(ctx, app, "Hollow Knight")
        .into_iter()
        .max_by(|a, b| a.y.total_cmp(&b.y))
        .expect("the card title is painted")
}

#[test]
fn app_renders_headless_and_emits_navigation_actions() {
    let ctx = Context::default();
    let mut app = app_with(vec![hollow_knight()]);

    run_frames(&ctx, &mut app, vec![Vec::new()]);

    assert!(app.take_actions().is_empty());
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
fn connect_drive_uses_stored_credentials_and_falls_back_to_setup_when_blank() {
    let with_credentials = |client_id: &str, client_secret: &str| AppUiState {
        config: Some(config_with(
            Vec::new(),
            Some(Credentials {
                client_id: client_id.to_string(),
                client_secret: client_secret.to_string(),
                refresh_token: None,
            }),
        )),
        ..AppUiState::default()
    };

    assert_eq!(
        connect_drive_action(&with_credentials("id", "secret")),
        Some(UiAction::ConnectDrive {
            client_id: "id".to_string(),
            client_secret: "secret".to_string(),
        })
    );
    // Kotlin `isBlank`: whitespace-only values open the setup dialog instead.
    assert_eq!(connect_drive_action(&with_credentials("id", "  ")), None);
    assert_eq!(
        connect_drive_action(&with_credentials("\t", "secret")),
        None
    );
    assert_eq!(connect_drive_action(&AppUiState::default()), None);
}

#[test]
fn clicking_a_card_launches_that_game_once() {
    let ctx = Context::default();
    let mut app = app_with(vec![hollow_knight()]);
    let title = card_title(&ctx, &mut app);
    app.take_actions();

    run_frames(&ctx, &mut app, click_at(title));

    assert_eq!(
        app.take_actions(),
        vec![UiAction::LaunchGame(hollow_knight_item())]
    );
    assert_eq!(app.active_dialog(), &ActiveDialog::None);
}

#[test]
fn card_edit_button_opens_edit_without_launching() {
    let ctx = Context::default();
    let mut app = app_with(vec![hollow_knight()]);
    let title = card_title(&ctx, &mut app);
    hover(&ctx, &mut app, title);
    let pencil = find_glyph_centers(&ctx, &mut app, PENCIL)
        .into_iter()
        .max_by(|a, b| a.y.total_cmp(&b.y))
        .expect("the card shows its edit button while hovered");
    app.take_actions();

    run_frames(&ctx, &mut app, click_at(pencil));

    assert_eq!(
        app.take_actions(),
        vec![UiAction::EditGame(hollow_knight_item())]
    );
}

#[test]
fn main_view_ignores_clicks_while_any_status_or_dialog_is_modal() {
    type Modal = (&'static str, fn(&mut EmuSyncApp));
    let modals: [Modal; 5] = [
        ("playing", |app| app.state.status = AppStatus::Playing),
        ("loading", |app| app.state.status = AppStatus::Loading),
        ("syncing", |app| {
            app.state.status = AppStatus::Syncing {
                message: "Uploading".to_string(),
            }
        }),
        ("error", |app| {
            app.state.status = AppStatus::Error {
                message: "Boom".to_string(),
            }
        }),
        ("confirm delete", |app| app.request_delete_entry(gba())),
    ];

    // Where the card and the other sidebar entry sit, measured without a modal.
    let ctx = Context::default();
    let mut probe = app_with(vec![hollow_knight(), gba()]);
    let card = card_title(&ctx, &mut probe);
    let sidebar_gba = center_of_text(&ctx, &mut probe, "GBA");

    for (name, activate) in modals {
        // One click per app: a dismissible modal closes on it, and a later click would
        // then be an ordinary click on the main view.
        for (target, pos) in [("card", card), ("sidebar entry", sidebar_gba)] {
            let ctx = Context::default();
            let mut app = app_with(vec![hollow_knight(), gba()]);
            settle(&ctx, &mut app);
            activate(&mut app);
            settle(&ctx, &mut app);
            app.take_actions();

            run_frames(&ctx, &mut app, click_at(pos));

            let actions = app.take_actions();
            assert!(
                actions
                    .iter()
                    .all(|action| matches!(action, UiAction::ClearStatus)),
                "{name} / {target}: {actions:?}"
            );
            assert_eq!(
                app.state.selected_entry,
                Some(hollow_knight()),
                "{name} / {target}"
            );
        }
    }
}

#[test]
fn sync_chip_maps_each_status_to_its_action() {
    let entry = hollow_knight();
    let with_status = |status: CloudSyncStatus| {
        let mut app = app_with(vec![entry.clone()]);
        app.state
            .entry_sync_status
            .insert("Hollow Knight".to_string(), status);
        app
    };

    for (status, label) in [
        (CloudSyncStatus::OutOfSync, "Out of sync • Sync All"),
        (CloudSyncStatus::Conflict, "Conflict • Sync"),
        (CloudSyncStatus::Idle, "Sync All"),
        (CloudSyncStatus::Error, "Sync All"),
    ] {
        let ctx = Context::default();
        let mut app = with_status(status);
        settle(&ctx, &mut app);
        app.take_actions();
        click_button(&ctx, &mut app, label);
        assert_eq!(
            app.take_actions(),
            vec![UiAction::SyncEntry(entry.clone())],
            "{status:?}"
        );
    }

    // In sync: the refresh glyph re-checks.
    let ctx = Context::default();
    let mut app = with_status(CloudSyncStatus::InSync);
    settle(&ctx, &mut app);
    let refresh = find_glyph_centers(&ctx, &mut app, REFRESH);
    assert_eq!(refresh.len(), 1);
    app.take_actions();
    run_frames(&ctx, &mut app, click_at(refresh[0]));
    assert_eq!(
        app.take_actions(),
        vec![UiAction::CheckSyncEntry(entry.clone())]
    );

    // No sync can start while checking or syncing, or when it is not configured.
    for status in [
        CloudSyncStatus::Checking,
        CloudSyncStatus::Syncing,
        CloudSyncStatus::NotConfigured,
    ] {
        let ctx = Context::default();
        let mut app = with_status(status);
        settle(&ctx, &mut app);
        for label in ["Sync All", "Out of sync • Sync All", "Conflict • Sync"] {
            assert!(!text_exists(&ctx, &mut app, label), "{status:?}: {label}");
        }
        assert!(
            find_glyph_centers(&ctx, &mut app, REFRESH).is_empty(),
            "{status:?}"
        );
        app.take_actions();
        if let Some(busy) = ["Syncing...", "Checking..."]
            .into_iter()
            .find_map(|label| text_center(&ctx, &mut app, label))
        {
            run_frames(&ctx, &mut app, click_at(busy));
        }
        assert_eq!(app.take_actions(), Vec::<UiAction>::new(), "{status:?}");
    }
}

#[test]
fn connect_drive_click_connects_directly_or_opens_setup() {
    let credentials_app = |credentials: Credentials| {
        let mut app = EmuSyncApp::new();
        app.state.config = Some(config_with(Vec::new(), Some(credentials)));
        app
    };

    let ctx = Context::default();
    let mut app = credentials_app(complete_credentials(None, "secret"));
    settle(&ctx, &mut app);
    click_button(&ctx, &mut app, "Connect Drive");
    assert_eq!(
        app.take_actions(),
        vec![UiAction::ConnectDrive {
            client_id: "id".to_string(),
            client_secret: "secret".to_string(),
        }]
    );
    assert_eq!(app.active_dialog(), &ActiveDialog::None);

    let ctx = Context::default();
    let mut app = credentials_app(complete_credentials(None, "  "));
    settle(&ctx, &mut app);
    click_button(&ctx, &mut app, "Connect Drive");
    assert_eq!(app.take_actions(), Vec::<UiAction>::new());
    assert!(matches!(app.active_dialog(), ActiveDialog::DriveSetup(_)));

    let ctx = Context::default();
    let mut app = credentials_app(complete_credentials(Some("1//token"), "secret"));
    settle(&ctx, &mut app);
    click_button(&ctx, &mut app, "Drive Linked");
    assert_eq!(app.take_actions(), Vec::<UiAction>::new());
    assert!(matches!(app.active_dialog(), ActiveDialog::DriveSetup(_)));
}

#[test]
fn safe_close_and_check_updates_emit_their_actions() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    settle(&ctx, &mut app);
    click_button(&ctx, &mut app, "Safe Close");
    assert_eq!(app.take_actions(), vec![UiAction::Exit]);

    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.state.update_state = UpdateState::Idle;
    settle(&ctx, &mut app);
    click_button(&ctx, &mut app, "Check Updates");
    assert_eq!(
        app.take_actions(),
        vec![UiAction::CheckForUpdates { manual: true }]
    );

    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.state.update_state = UpdateState::Checking;
    settle(&ctx, &mut app);
    assert!(!text_exists(&ctx, &mut app, "Check Updates"));
}

#[test]
fn sidebar_click_selects_and_updates_state_immediately() {
    let ctx = Context::default();
    let mut app = app_with(vec![hollow_knight(), gba()]);
    settle(&ctx, &mut app);
    app.take_actions();

    click_button(&ctx, &mut app, "GBA");

    assert_eq!(app.take_actions(), vec![UiAction::SelectEntry(Some(gba()))]);
    assert_eq!(app.state.selected_entry, Some(gba()));
}

#[test]
fn sidebar_reorder_buttons_respect_bounds() {
    let (a, b) = (native_entry("A"), native_entry("B"));
    let library = || {
        let mut app = EmuSyncApp::new();
        app.state.config = Some(config_with(vec![a.clone(), b.clone()], None));
        app
    };

    let ctx = Context::default();
    let mut app = library();
    settle(&ctx, &mut app);
    let row_a = center_of_text(&ctx, &mut app, "A");
    hover(&ctx, &mut app, row_a);
    assert!(
        find_glyph_centers(&ctx, &mut app, CHEVRON_UP).is_empty(),
        "the first entry cannot move up"
    );
    let down = find_glyph_centers(&ctx, &mut app, CHEVRON_DOWN);
    assert_eq!(down.len(), 1);
    app.take_actions();
    run_frames(&ctx, &mut app, click_at(down[0]));
    assert_eq!(
        app.take_actions(),
        vec![UiAction::ReorderEntries(vec![b.clone(), a.clone()])]
    );

    let ctx = Context::default();
    let mut app = library();
    settle(&ctx, &mut app);
    let row_b = center_of_text(&ctx, &mut app, "B");
    hover(&ctx, &mut app, row_b);
    assert!(
        find_glyph_centers(&ctx, &mut app, CHEVRON_DOWN).is_empty(),
        "the last entry cannot move down"
    );
    assert_eq!(find_glyph_centers(&ctx, &mut app, CHEVRON_UP).len(), 1);
}

#[test]
fn steam_toggle_requires_steam_available() {
    let entry = native_entry("A");
    for available in [true, false] {
        let ctx = Context::default();
        let mut app = EmuSyncApp::new();
        app.state.steam_available = available;
        app.state.config = Some(config_with(vec![entry.clone()], None));
        settle(&ctx, &mut app);
        let toggles = find_glyph_centers(&ctx, &mut app, WIDGETS);
        app.take_actions();

        if available {
            assert_eq!(toggles.len(), 1);
            run_frames(&ctx, &mut app, click_at(toggles[0]));
            assert_eq!(
                app.take_actions(),
                vec![UiAction::ToggleSteam(entry.clone())]
            );
        } else {
            assert!(toggles.is_empty());
        }
    }
}

#[test]
fn stale_drag_ends_when_pointer_is_released() {
    let ctx = Context::default();
    let mut app = app_with(vec![hollow_knight()]);
    app.sidebar.start_drag("entry_Missing".to_string());
    assert!(app.sidebar.dragging_key().is_some());

    run_frames(&ctx, &mut app, vec![Vec::new()]);

    assert_eq!(app.sidebar.dragging_key(), None);
    assert_eq!(app.sidebar.drag_offset_y(), 0.0);
    assert_eq!(app.take_actions(), Vec::<UiAction>::new());
}
