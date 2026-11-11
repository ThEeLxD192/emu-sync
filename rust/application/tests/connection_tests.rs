use emusync_application::CloudConnection;
use emusync_domain::config::{AppConfig, CloudAuthorization, ConfigurationStore, Credentials};
use emusync_domain::library::{EntryCommon, GameEntry, NativeGame};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct MockStore {
    config: Arc<Mutex<AppConfig>>,
    fail_load: Arc<AtomicBool>,
}

impl MockStore {
    fn new(config: AppConfig) -> Self {
        Self {
            config: Arc::new(Mutex::new(config)),
            fail_load: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl ConfigurationStore for MockStore {
    type Error = std::io::Error;

    fn load(&self) -> Result<AppConfig, Self::Error> {
        if self.fail_load.load(Ordering::SeqCst) {
            return Err(std::io::Error::other("disk read failure"));
        }
        Ok(self.config.lock().unwrap().clone())
    }

    fn update(
        &self,
        transform: impl FnOnce(AppConfig) -> AppConfig,
    ) -> Result<AppConfig, Self::Error> {
        let mut lock = self.config.lock().unwrap();
        *lock = transform(lock.clone());
        Ok(lock.clone())
    }

    fn update_credentials(
        &self,
        transform: impl FnOnce(Option<Credentials>) -> Option<Credentials>,
    ) -> Result<AppConfig, Self::Error> {
        let mut lock = self.config.lock().unwrap();
        let current = lock.credentials.clone();
        lock.credentials = transform(current);
        Ok(lock.clone())
    }
}

struct MockAuth {
    should_succeed: bool,
}

impl CloudAuthorization for MockAuth {
    type Error = String;

    async fn authorize(&self, _cfg: &AppConfig, _interactive: bool) -> Result<String, Self::Error> {
        if self.should_succeed {
            Ok("mock_auth_token".into())
        } else {
            Err("Simulated OAuth refusal".into())
        }
    }
}

#[tokio::test]
async fn login_returns_authorized_when_oauth_succeeds() {
    let cfg = AppConfig {
        credentials: Some(Credentials {
            client_id: "client123".into(),
            client_secret: "secret456".into(),
            refresh_token: None,
        }),
        ..Default::default()
    };

    let store = MockStore::new(cfg.clone());
    let auth = MockAuth {
        should_succeed: true,
    };
    let connection = CloudConnection::new(store, auth);

    let result = connection.login(Some(&cfg)).await;

    assert!(result.authorized);
    assert_eq!(result.message, "Google Drive successfully connected!");
    assert!(result.config.is_some());
}

#[tokio::test]
async fn login_returns_unauthorized_when_oauth_fails() {
    let cfg = AppConfig {
        credentials: Some(Credentials {
            client_id: "client123".into(),
            client_secret: "secret456".into(),
            refresh_token: None,
        }),
        ..Default::default()
    };

    let store = MockStore::new(cfg.clone());
    let auth = MockAuth {
        should_succeed: false,
    };
    let connection = CloudConnection::new(store, auth);

    let result = connection.login(Some(&cfg)).await;

    assert!(!result.authorized);
    assert_eq!(
        result.message,
        "Google Drive login failed: Simulated OAuth refusal"
    );
    assert!(result.config.is_some());
}

#[tokio::test]
async fn login_validates_presence_of_config_and_credentials() {
    let store = MockStore::new(AppConfig::default());
    let auth = MockAuth {
        should_succeed: true,
    };
    let connection = CloudConnection::new(store, auth);

    let no_config = connection.login(None).await;
    assert!(!no_config.authorized);
    assert_eq!(no_config.message, "No config loaded");

    let empty_cfg = AppConfig::default();
    let no_credentials = connection.login(Some(&empty_cfg)).await;
    assert!(!no_credentials.authorized);
    assert_eq!(
        no_credentials.message,
        "Missing googleDrive configuration in config.json"
    );
}

#[test]
fn setup_saves_credentials_and_preserves_matching_refresh_token() {
    let initial_cfg = AppConfig {
        credentials: Some(Credentials {
            client_id: "existing_id".into(),
            client_secret: "existing_secret".into(),
            refresh_token: Some("existing_token".into()),
        }),
        ..Default::default()
    };

    let store = MockStore::new(initial_cfg.clone());
    let auth = MockAuth {
        should_succeed: true,
    };
    let connection = CloudConnection::new(store, auth);

    // Same credentials: keep refresh_token
    let res = connection
        .setup(Some(&initial_cfg), "existing_id", "existing_secret")
        .unwrap();
    let creds = res.config.unwrap().credentials.unwrap();
    assert_eq!(creds.client_id, "existing_id");
    assert_eq!(creds.refresh_token.as_deref(), Some("existing_token"));

    // Different credentials: reset refresh_token to None
    let res_changed = connection
        .setup(Some(&initial_cfg), "new_id", "new_secret")
        .unwrap();
    let creds_changed = res_changed.config.unwrap().credentials.unwrap();
    assert_eq!(creds_changed.client_id, "new_id");
    assert_eq!(creds_changed.refresh_token, None);

    // Blank credentials rejected
    let res_blank = connection
        .setup(Some(&initial_cfg), "   ", "new_secret")
        .unwrap();
    assert_eq!(
        res_blank.message,
        "Client ID and Client Secret cannot be blank."
    );
}

#[test]
fn unlink_clears_refresh_token() {
    let initial_cfg = AppConfig {
        credentials: Some(Credentials {
            client_id: "existing_id".into(),
            client_secret: "existing_secret".into(),
            refresh_token: Some("existing_token".into()),
        }),
        ..Default::default()
    };

    let store = MockStore::new(initial_cfg.clone());
    let auth = MockAuth {
        should_succeed: true,
    };
    let connection = CloudConnection::new(store, auth);

    let res = connection.unlink(Some(&initial_cfg)).unwrap();
    assert_eq!(res.message, "Google Drive disconnected.");
    let creds = res.config.unwrap().credentials.unwrap();
    assert_eq!(creds.refresh_token, None);
    assert_eq!(creds.client_id, "existing_id");
}

#[tokio::test]
async fn login_falls_back_to_input_config_when_store_reload_fails_and_credential_updates_preserve_concurrent_library_entries()
 {
    let input_cfg = AppConfig {
        credentials: Some(Credentials {
            client_id: "client123".into(),
            client_secret: "secret456".into(),
            refresh_token: None,
        }),
        entries: Vec::new(),
    };

    // Subcase A: Store fails to load after OAuth authorization succeeds
    {
        let store = MockStore::new(input_cfg.clone());
        let auth = MockAuth {
            should_succeed: true,
        };
        store.fail_load.store(true, Ordering::SeqCst);
        let connection = CloudConnection::new(store, auth);

        let result = connection.login(Some(&input_cfg)).await;
        assert!(result.authorized);
        assert_eq!(result.message, "Google Drive successfully connected!");
        assert_eq!(result.config, Some(input_cfg.clone()));
    }

    // Subcase B: Credential updates (setup / unlink) preserve concurrent library entries
    {
        let store = MockStore::new(AppConfig::default());
        let store_clone = store.clone();
        let auth = MockAuth {
            should_succeed: true,
        };
        let connection = CloudConnection::new(store, auth);

        // A stale config snapshot taken before a concurrent addition (has 0 entries)
        let stale_snapshot = AppConfig::default();

        // Concurrently, another subsystem adds an entry to the store
        let concurrent_entry = GameEntry::Native(NativeGame {
            common: EntryCommon {
                name: "ConcurrentMetroid".to_string(),
                executable_path: "/games/metroid".to_string(),
                ..EntryCommon::default()
            },
            wait_for_process: None,
            cover_path: None,
        });

        store_clone
            .update(|mut cfg| {
                cfg.entries.push(concurrent_entry.clone());
                cfg
            })
            .unwrap();

        // Now setup runs with the stale snapshot
        let setup_res = connection
            .setup(Some(&stale_snapshot), "client_1", "secret_1")
            .unwrap();

        let cfg = setup_res.config.unwrap();
        assert_eq!(cfg.entries.len(), 1);
        assert_eq!(cfg.entries[0].common().name, "ConcurrentMetroid");
        assert_eq!(cfg.credentials.as_ref().unwrap().client_id, "client_1");

        // Verify loaded store also preserves the concurrent entry
        let loaded = connection.load_stored_connection().unwrap();
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].common().name, "ConcurrentMetroid");

        // Now test unlink with a stale snapshot that has credentials but 0 entries:
        let stale_with_creds = AppConfig {
            credentials: Some(Credentials {
                client_id: "client_1".into(),
                client_secret: "secret_1".into(),
                refresh_token: Some("existing_token".into()),
            }),
            entries: Vec::new(),
        };
        let unlink_res = connection.unlink(Some(&stale_with_creds)).unwrap();
        let unlinked_cfg = unlink_res.config.unwrap();
        assert_eq!(unlinked_cfg.entries.len(), 1);
        assert_eq!(unlinked_cfg.entries[0].common().name, "ConcurrentMetroid");
        assert_eq!(
            unlinked_cfg.credentials.as_ref().unwrap().refresh_token,
            None
        );
    }
}
