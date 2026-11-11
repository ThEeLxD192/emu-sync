use emusync_application::{EntrySyncChecks, OperationCoordinator};
use emusync_domain::config::{AppConfig, Credentials};
use emusync_domain::library::{EntryCommon, GameEntry, NativeGame};
use emusync_domain::sync::{
    Decision, EntrySyncStatus, PostSyncFailure, SaveConflict, SavePlacementRequest, SaveSetup,
    SaveSynchronizer, SaveSynchronizerFactory, SyncEvent, SyncGame,
};
use std::sync::Arc;

struct MockSynchronizer;

impl SaveSynchronizer for MockSynchronizer {
    type Error = String;

    async fn check_entry(&self, _entry: &GameEntry) -> Result<EntrySyncStatus, Self::Error> {
        Ok(EntrySyncStatus::InSync)
    }

    async fn sync_entry(&self, _entry: &GameEntry) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn rename_game(
        &self,
        _entry: &GameEntry,
        _rom_name: Option<&str>,
        _old_title: &str,
        _new_title: &str,
        _local_save_targets: &[emusync_domain::library::SaveTarget],
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn rename_entry(&self, _old: &GameEntry, _new: &GameEntry) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn play_with_sync(
        &self,
        _game: SyncGame,
        _setup: &(impl SaveSetup + Sync),
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn resolve_conflict(&self, _request: &SaveConflict, _decision: Decision) {}

    fn resolve_save_placement(&self, _request: &SavePlacementRequest, _choice: Option<Vec<usize>>) {
    }

    async fn retry_post_sync(&self, _failure: &PostSyncFailure) -> Result<(), Self::Error> {
        Ok(())
    }
}

struct MockFactory;

impl SaveSynchronizerFactory for MockFactory {
    type Synchronizer = MockSynchronizer;

    fn create(
        &self,
        _config: AppConfig,
        _on_status: impl Fn(SyncEvent) + Send + Sync + 'static,
    ) -> Self::Synchronizer {
        MockSynchronizer
    }
}

fn sample_entry(name: &str) -> GameEntry {
    GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: name.to_string(),
            executable_path: format!("/games/{name}"),
            ..EntryCommon::default()
        },
        wait_for_process: None,
        cover_path: None,
    })
}

#[tokio::test]
async fn sync_check_freshness_invalidation_matrix() {
    let operations = Arc::new(OperationCoordinator::new());
    let checks = EntrySyncChecks::new(Arc::clone(&operations), MockFactory);

    let entry = sample_entry("Metroid");
    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "client-id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token-1".into()),
        }),
        entries: vec![entry.clone()],
    };

    let req = checks.begin(&entry, &config);

    // 1. Initially valid
    assert!(checks.is_current(&req, Some(&config)));

    // 2. Read returns mock synchronizer status
    let status = checks.read(&req).await.unwrap();
    assert_eq!(status, EntrySyncStatus::InSync);

    // 3. Stale when operation revision changes
    let _ = operations.run_if_idle(async { "op" }).await;
    assert!(!checks.is_current(&req, Some(&config)));

    // New check with current revision
    let req2 = checks.begin(&entry, &config);
    assert!(checks.is_current(&req2, Some(&config)));

    // 4. Stale when credentials change (e.g. disconnect or new account)
    let mut config_new_creds = config.clone();
    config_new_creds.credentials = Some(Credentials {
        client_id: "client-id".into(),
        client_secret: "secret".into(),
        refresh_token: Some("token-2".into()),
    });
    assert!(!checks.is_current(&req2, Some(&config_new_creds)));

    // 5. Stale when entry is removed from current configuration
    let config_without_entry = AppConfig {
        credentials: config.credentials.clone(),
        entries: Vec::new(),
    };
    assert!(!checks.is_current(&req2, Some(&config_without_entry)));

    // 6. Stale when current is None (unloaded config)
    assert!(!checks.is_current(&req2, None));

    // 7. Stale when a newer check begins for the same game
    let req3 = checks.begin(&entry, &config);
    assert!(!checks.is_current(&req2, Some(&config)));
    assert!(checks.is_current(&req3, Some(&config)));

    // 8. Stale when entry is edited in-place in current configuration
    let mut modified_entry = entry.clone();
    if let GameEntry::Native(ref mut n) = modified_entry {
        n.common.executable_path = "/games/Metroid_v2".into();
    }
    let config_with_edited_entry = AppConfig {
        credentials: config.credentials.clone(),
        entries: vec![modified_entry],
    };
    assert!(!checks.is_current(&req3, Some(&config_with_edited_entry)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_check_stale_while_operation_is_busy_and_supports_independent_entries() {
    let operations = Arc::new(OperationCoordinator::new());
    let checks = EntrySyncChecks::new(Arc::clone(&operations), MockFactory);

    let game_a = sample_entry("Zelda");
    let game_b = sample_entry("Mario");

    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "c".into(),
            client_secret: "s".into(),
            refresh_token: Some("rt".into()),
        }),
        entries: vec![game_a.clone(), game_b.clone()],
    };

    let req_a = checks.begin(&game_a, &config);
    let req_b = checks.begin(&game_b, &config);

    // Both are valid initially and independent
    assert!(checks.is_current(&req_a, Some(&config)));
    assert!(checks.is_current(&req_b, Some(&config)));

    // A new check for game A only invalidates game A's previous check, NOT game B
    let req_a2 = checks.begin(&game_a, &config);
    assert!(!checks.is_current(&req_a, Some(&config)));
    assert!(checks.is_current(&req_a2, Some(&config)));
    assert!(checks.is_current(&req_b, Some(&config)));

    // While an operation is actively running (is_busy == true), checks are stale
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (finish_tx, finish_rx) = tokio::sync::oneshot::channel();

    let ops_clone = Arc::clone(&operations);
    let task = tokio::spawn(async move {
        ops_clone
            .run_if_idle(async move {
                let _ = started_tx.send(());
                let _ = finish_rx.await;
            })
            .await
    });

    started_rx.await.unwrap();
    assert!(operations.is_busy());

    // Both checks must report stale because the system is busy
    assert!(!checks.is_current(&req_a2, Some(&config)));
    assert!(!checks.is_current(&req_b, Some(&config)));

    // Release operation
    let _ = finish_tx.send(());
    let _ = task.await;

    assert!(!operations.is_busy());
    // Once operation completes, revision also changed so previous checks remain stale
    assert!(!checks.is_current(&req_a2, Some(&config)));
    assert!(!checks.is_current(&req_b, Some(&config)));

    // A fresh check after the operation completes is valid
    let req_b2 = checks.begin(&game_b, &config);
    assert!(checks.is_current(&req_b2, Some(&config)));
}
