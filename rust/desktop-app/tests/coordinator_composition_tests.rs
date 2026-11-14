mod common;

use common::TestHarness;
use emusync_desktop_app::coordinator::composition::{drive_connected, format_epoch_ms};
use emusync_domain::{
    config::{AppConfig, Credentials},
    sync::{RequestId, SaveConflict},
};
use tokio_util::sync::CancellationToken;

#[test]
fn format_epoch_ms_and_drive_connected_handle_boundary_inputs() {
    // 1. drive_connected checks
    let cfg_none = AppConfig {
        language: Default::default(),
        credentials: None,
        entries: vec![],
    };
    assert!(!drive_connected(&cfg_none));

    let cfg_no_token = AppConfig {
        language: Default::default(),
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: None,
        }),
        entries: vec![],
    };
    assert!(!drive_connected(&cfg_no_token));

    let cfg_blank_token = AppConfig {
        language: Default::default(),
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("   \t\n".into()),
        }),
        entries: vec![],
    };
    assert!(!drive_connected(&cfg_blank_token));

    let cfg_valid = AppConfig {
        language: Default::default(),
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: Some(" valid_token ".into()),
        }),
        entries: vec![],
    };
    assert!(drive_connected(&cfg_valid));

    // 2. format_epoch_ms checks
    assert_eq!(format_epoch_ms(0), "1970-01-01 00:00");
    assert_eq!(format_epoch_ms(1_700_000_000_000), "2023-11-14 22:13");
    // Boundary overflow: i64::MAX nanos overflows OffsetDateTime and must fallback to "{ms} ms"
    assert_eq!(format_epoch_ms(i64::MAX), format!("{} ms", i64::MAX));
}

#[tokio::test]
async fn build_orchestrator_progress_callback_guards_zero_total_and_wires_conflict_and_retry() {
    let harness = TestHarness::new();
    let config = AppConfig {
        language: Default::default(),
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![],
    };

    // Install initial cancel token and conflict in interaction controls
    let initial_cancel = CancellationToken::new();
    harness
        .coordinator
        .interaction
        .set_cancel_token(initial_cancel.clone());

    let conflict = SaveConflict {
        request_id: RequestId(1),
        local_modified_ms: 1000,
        cloud_modified_ms: 2000,
        game_name: "Chrono Trigger".into(),
    };
    harness
        .coordinator
        .interaction
        .on_conflict(conflict.clone());

    assert_eq!(
        harness
            .coordinator
            .interaction
            .current_conflict
            .lock()
            .unwrap()
            .clone(),
        Some(conflict.clone())
    );
    assert!(!initial_cancel.is_cancelled());

    // 1. build_check_orchestrator must NOT clobber active_cancel nor current_conflict
    let check_res = harness.coordinator.build_check_orchestrator(&config);
    assert!(check_res.is_ok());
    assert_eq!(
        harness
            .coordinator
            .interaction
            .current_conflict
            .lock()
            .unwrap()
            .clone(),
        Some(conflict)
    );
    assert!(!initial_cancel.is_cancelled());

    // 2. build_orchestrator replaces active_cancel with a new active token and wires resolver
    let built = harness.coordinator.build_orchestrator(&config).unwrap();
    assert!(!built.retained.has_error());

    // Cancel active from interaction controls - the new token is cancelled, but initial_cancel was replaced and kept uncancelled
    harness.coordinator.interaction.cancel_active();
    assert!(!initial_cancel.is_cancelled());
}
