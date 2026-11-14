use crate::{
    guarded_files::GuardedFiles,
    retained_files::{FailureClass, RetainedFiles},
};
use emusync_application::sync::{SyncOrchestrator, SyncServices};
use emusync_data_drive::{
    DriveFolders, DriveHttpClients, DriveListing, DriveTransfer, InteractiveOAuth, TransferProgress,
};
use emusync_data_local::{
    ConfigManager, DesktopSaveAccess, RecoveryCredentials, RecoveryStore, SyncHistoryStore,
    TrackedError, TrackedFileSync,
};
use emusync_domain::{
    config::{AppConfig, CredentialStore},
    sync::{SaveSynchronizerFactory, SyncEvent},
};
use emusync_platform_desktop::{DesktopBrowserLauncher, DesktopSteamLibrary, GameRunner};
use emusync_presentation::i18n::fill;
use reqwest::Url;
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;

pub fn format_epoch_ms(ms: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .map(|dt| {
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}",
                dt.year(),
                dt.month() as u8,
                dt.day(),
                dt.hour(),
                dt.minute()
            )
        })
        .unwrap_or_else(|_| format!("{ms} ms"))
}

pub fn drive_connected(config: &AppConfig) -> bool {
    config
        .credentials
        .as_ref()
        .and_then(|credentials| credentials.refresh_token.as_deref())
        .is_some_and(|token| !token.trim().is_empty())
}

/// Endpoint configuration for Drive and OAuth, allowing mock injection during tests.
#[derive(Clone, Debug)]
pub struct DriveEndpoints {
    pub files_v3: Url,
    pub upload_v3: Url,
    pub files_v2: Url,
    pub upload_v2: Url,
    pub token_endpoint: Url,
    pub authorization_endpoint: Url,
    pub oauth_address: SocketAddr,
    pub redirect_uri: String,
}

impl Default for DriveEndpoints {
    fn default() -> Self {
        Self {
            files_v3: Url::parse("https://www.googleapis.com/drive/v3/files").expect("static URL"),
            upload_v3: Url::parse("https://www.googleapis.com/upload/drive/v3/files")
                .expect("static URL"),
            files_v2: Url::parse("https://www.googleapis.com/drive/v2/files").expect("static URL"),
            upload_v2: Url::parse("https://www.googleapis.com/upload/drive/v2/files")
                .expect("static URL"),
            token_endpoint: Url::parse("https://oauth2.googleapis.com/token").expect("static URL"),
            authorization_endpoint: Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
                .expect("static URL"),
            oauth_address: "127.0.0.1:8089".parse().expect("static address"),
            redirect_uri: emusync_data_drive::REDIRECT_URI.to_string(),
        }
    }
}

pub type DesktopSyncOrchestrator = SyncOrchestrator<
    DesktopSaveAccess,
    DriveListing,
    DriveFolders,
    RetainedFiles<GuardedFiles<DriveTransfer, ConfigManager>>,
    InteractiveOAuth<ConfigManager, DesktopBrowserLauncher>,
    GameRunner<DesktopSteamLibrary>,
>;

pub type DesktopRetainedFiles = RetainedFiles<GuardedFiles<DriveTransfer, ConfigManager>>;

pub struct BuiltOrchestrator {
    pub orchestrator: Arc<DesktopSyncOrchestrator>,
    pub retained: DesktopRetainedFiles,
    pub credentials: RecoveryCredentials<ConfigManager>,
    pub last_event: Arc<Mutex<Option<SyncEvent>>>,
}

#[allow(clippy::too_many_arguments)]
pub fn build_orchestrator(
    t: &emusync_presentation::i18n::Texts,
    clients: DriveHttpClients,
    config_manager: &ConfigManager,
    steam: &DesktopSteamLibrary,
    browser: &Arc<DesktopBrowserLauncher>,
    endpoints: &DriveEndpoints,
    config: &AppConfig,
    cancel: CancellationToken,
    transfer_progress: Arc<dyn Fn(TransferProgress) + Send + Sync>,
    on_status: impl Fn(SyncEvent) + Send + Sync + 'static,
) -> Result<BuiltOrchestrator, String> {
    let listing = DriveListing::new(clients.metadata.clone(), endpoints.files_v3.clone());
    let folders = DriveFolders::new(clients.metadata.clone(), endpoints.files_v3.clone());

    let drive_transfer = DriveTransfer::with_clients(
        clients.clone(),
        endpoints.files_v3.clone(),
        endpoints.upload_v3.clone(),
        endpoints.files_v2.clone(),
        endpoints.upload_v2.clone(),
        cancel,
        transfer_progress,
    );

    let history = Arc::new(
        SyncHistoryStore::new(config_manager.path())
            .map_err(|e| fill(t.msg_history_open_failed, &[("error", &e)]))?,
    );
    let recovery = Arc::new(
        RecoveryStore::new(config_manager.path())
            .map_err(|e| fill(t.msg_recovery_open_failed, &[("error", &e)]))?,
    );
    let tracked = Arc::new(TrackedFileSync::new(drive_transfer, history).with_recovery(recovery));

    let snapshot = config_manager
        .snapshot(config)
        .map_err(|e| fill(t.msg_credentials_snapshot_failed, &[("error", &e)]))?;
    let credentials =
        RecoveryCredentials::new(Arc::new(config_manager.clone()), snapshot, config.clone());

    let guarded = GuardedFiles::new(
        tracked,
        credentials.clone(),
        |e: &emusync_data_drive::TransferError| e.leaves_remote_unchanged(),
    );
    // Only an upload leaves an outcome to recover. A failed download or any other local
    // error changes nothing uncertain, so it is reported without blocking the app.
    let retained = RetainedFiles::with_classifier(guarded, |error: &TrackedError<_>| match error {
        TrackedError::Upload { .. } | TrackedError::AfterUpload { .. } => {
            FailureClass::NeedsRecovery
        }
        other => FailureClass::Plain(other.to_string()),
    });

    let oauth = InteractiveOAuth::with_endpoints(
        clients.metadata,
        config_manager.clone(),
        browser.clone(),
        endpoints.token_endpoint.clone(),
        endpoints.authorization_endpoint.clone(),
        endpoints.oauth_address,
        endpoints.redirect_uri.clone(),
    );

    let runner = GameRunner::new(Arc::new(steam.clone()));

    let services = SyncServices {
        files: DesktopSaveAccess,
        search: listing,
        folders,
        tracked_sync: retained.clone(),
        authorization: oauth,
        runner,
    };

    let last_event: Arc<Mutex<Option<SyncEvent>>> = Arc::new(Mutex::new(None));
    let last_event_inner = last_event.clone();
    let wrapped_on_status = move |event: SyncEvent| {
        *last_event_inner.lock().unwrap() = Some(event.clone());
        on_status(event);
    };

    let orchestrator = Arc::new(
        emusync_application::sync::SyncOrchestratorFactory::new(services)
            .create(config.clone(), wrapped_on_status),
    );

    Ok(BuiltOrchestrator {
        orchestrator,
        retained,
        credentials,
        last_event,
    })
}
