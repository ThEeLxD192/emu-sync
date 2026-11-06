use emusync_application::{EntrySyncChecks, OperationCoordinator};
use emusync_domain::config::{AppConfig, Credentials};
use emusync_domain::library::{EntryCommon, GameEntry, NativeGame};
use emusync_domain::sync::{
    Decision, EntrySyncStatus, PostSyncFailure, SaveConflict, SaveSetup, SaveSynchronizer,
    SaveSynchronizerFactory, SyncEvent, SyncGame,
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
}
