//! Status overlays: passive operations, conflict choice, post-sync failure and errors.

mod common;

use common::*;
use egui::Context;
use emusync_domain::sync::{Decision, SavePlacementGroup};
use emusync_presentation::{
    AppStatus, EmuSyncApp, UiAction,
    dialogs::{conflict_card_id, post_sync_failed_message, save_folder_option_id},
};

fn english() -> &'static emusync_presentation::i18n::Texts {
    emusync_presentation::i18n::texts(emusync_presentation::i18n::Language::English)
}

fn conflict() -> AppStatus {
    AppStatus::Conflict {
        local_date: "today".to_string(),
        cloud_date: "yesterday".to_string(),
        game_name: "Celeste".to_string(),
    }
}

fn post_sync_failed() -> AppStatus {
    AppStatus::PostSyncFailed {
        game_name: "Celeste".to_string(),
        reason: "Network unavailable".to_string(),
        completed_save_count: 1,
        pending_save_count: 2,
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
        conflict(),
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
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.state.status = conflict();
    // Clicks land dead center, on the card title.
    let local = settled_center_of(&ctx, &mut app, conflict_card_id("Local Save"));
    let cloud = settled_center_of(&ctx, &mut app, conflict_card_id("Cloud Save"));

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
    assert_eq!(
        post_sync_failed_message(english(), "Celeste", "Network unavailable", 0, 2),
        "Celeste closed normally. Google Drive was not updated.\n\nNetwork unavailable"
    );
    assert_eq!(
        post_sync_failed_message(english(), "Celeste", "Quota", 1, 2),
        "Celeste closed normally. 1 save(s) synced successfully; 2 save(s) still need to be uploaded.\n\nQuota"
    );
}

#[test]
fn answered_conflict_is_asked_again_after_any_other_status() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.state.status = conflict();
    let local = settled_center_of(&ctx, &mut app, conflict_card_id("Local Save"));
    run_frames(&ctx, &mut app, click_at(local));
    assert_eq!(
        app.take_actions(),
        vec![UiAction::ResolveConflict(Decision::UploadLocal)]
    );

    // Another status in between makes the app forget the answer, even if the very
    // same conflict comes back afterwards.
    app.state.status = AppStatus::Idle;
    run_frames(&ctx, &mut app, vec![Vec::new()]);
    app.state.status = conflict();

    let cloud = settled_center_of(&ctx, &mut app, conflict_card_id("Cloud Save"));
    run_frames(&ctx, &mut app, click_at(cloud));
    assert_eq!(
        app.take_actions(),
        vec![UiAction::ResolveConflict(Decision::DownloadCloud)]
    );
}

#[test]
fn post_sync_retry_emits_retry_and_keeps_the_notice() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.state.status = post_sync_failed();

    click_button(&ctx, &mut app, "Retry upload");

    assert_eq!(app.take_actions(), vec![UiAction::RetryPostSync]);
    assert_eq!(app.state.status, post_sync_failed());
}

#[test]
fn keep_local_clears_the_notice() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.state.status = post_sync_failed();

    click_button(&ctx, &mut app, "Keep local");

    assert_eq!(app.take_actions(), vec![UiAction::ClearStatus]);
    assert_eq!(app.state.status, AppStatus::Idle);
}

#[test]
fn error_close_button_clears_once() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.state.status = AppStatus::Error {
        message: "Drive quota exceeded".to_string(),
    };

    click_button(&ctx, &mut app, "Close");
    run_frames(&ctx, &mut app, vec![Vec::new(), Vec::new()]);

    assert_eq!(app.take_actions(), vec![UiAction::ClearStatus]);
    assert_eq!(app.state.status, AppStatus::Idle);
}

#[test]
fn clicking_the_title_or_date_of_a_conflict_card_chooses_it() {
    for (card, decision) in [
        ("Local Save", Decision::UploadLocal),
        ("Cloud Save", Decision::DownloadCloud),
    ] {
        for (part, dy) in [("title", 0.0), ("date", 22.0)] {
            let ctx = Context::default();
            let mut app = EmuSyncApp::new();
            app.state.status = conflict();
            let center = settled_center_of(&ctx, &mut app, conflict_card_id(card));
            let date_or_title = all_text_centers(&ctx, &mut app, card)[0] + egui::vec2(0.0, dy);
            assert!((date_or_title - center).length() < 60.0);
            app.take_actions();

            run_frames(&ctx, &mut app, click_at(date_or_title));

            assert_eq!(
                app.take_actions(),
                vec![UiAction::ResolveConflict(decision)],
                "{card} {part}"
            );
        }
    }
}

fn folder_choice() -> AppStatus {
    AppStatus::ChooseSaveFolders {
        game_name: "Crash Team Racing".to_string(),
        folders: vec![
            "/sd/Saves/savedata/1/CUSA13795".to_string(),
            "/sd/Saves/trophies/CUSA13795".to_string(),
        ],
        groups: vec![
            SavePlacementGroup {
                name: "savedata1".to_string(),
                files: vec!["savedata1/SYS-DATA.sav".to_string()],
                suggested_folder: Some(0),
            },
            SavePlacementGroup {
                name: "unknown.bin".to_string(),
                files: vec!["unknown.bin".to_string()],
                suggested_folder: None,
            },
        ],
    }
}

#[test]
fn folder_choice_needs_every_group_and_sends_the_picked_folders_once() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.state.status = folder_choice();

    // Escape does not dismiss it, and Restore waits for a folder for every group.
    run_frames(&ctx, &mut app, vec![Vec::new(), escape()]);
    click_button(&ctx, &mut app, "Restore");
    assert!(app.take_actions().is_empty());

    // The suggestion is only a preselection: it can be changed.
    for (group, folder) in [(1, 1), (0, 1)] {
        let option = settled_center_of(&ctx, &mut app, save_folder_option_id(group, folder));
        run_frames(&ctx, &mut app, click_at(option));
    }
    click_button(&ctx, &mut app, "Restore");
    assert_eq!(
        app.take_actions(),
        vec![UiAction::ResolveSaveFolders(Some(vec![1, 1]))]
    );

    // While the coordinator has not changed the status, no second answer can be sent.
    run_frames(&ctx, &mut app, vec![Vec::new(), Vec::new()]);
    assert!(ctx.read_response(save_folder_option_id(0, 0)).is_none());
    assert!(app.take_actions().is_empty());
}

#[test]
fn cancelling_the_folder_choice_answers_without_folders() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.state.status = folder_choice();

    click_button(&ctx, &mut app, "Cancel");

    assert_eq!(app.take_actions(), vec![UiAction::ResolveSaveFolders(None)]);
}
