use crate::events::AppEvent;
use emusync_application::library::game_specific_targets;
use emusync_data_local::LocalSaveTargets;
use emusync_domain::sync::{
    Decision, PostSyncFailure, SaveConflict, SaveSetup, SaveSetupRequest, SaveSetupResult, SyncGame,
};
use emusync_presentation::model::GameItem;
use std::{
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex, mpsc::Sender},
};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

pub type ConflictResolver = Arc<dyn Fn(&SaveConflict, Decision) + Send + Sync>;
pub type RetryRunner = Arc<
    dyn Fn(PostSyncFailure) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync + 'static,
>;

type PendingSaveSetupSlot =
    Arc<Mutex<Option<(String, oneshot::Sender<Result<SaveSetupResult, String>>)>>>;

#[derive(Clone)]
pub struct InteractionControls {
    pub event_tx: Sender<AppEvent>,
    pub redraw: Arc<dyn Fn() + Send + Sync>,
    pub current_conflict: Arc<Mutex<Option<SaveConflict>>>,
    pub active_conflict_resolver: Arc<Mutex<Option<ConflictResolver>>>,
    pub active_retry_failure: Arc<Mutex<Option<PostSyncFailure>>>,
    pub active_retry_runner: Arc<Mutex<Option<RetryRunner>>>,
    pub active_cancel: Arc<Mutex<Option<CancellationToken>>>,
    pub pending_save_setup: PendingSaveSetupSlot,
}

impl InteractionControls {
    pub fn new(event_tx: Sender<AppEvent>, redraw: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            event_tx,
            redraw,
            current_conflict: Arc::new(Mutex::new(None)),
            active_conflict_resolver: Arc::new(Mutex::new(None)),
            active_retry_failure: Arc::new(Mutex::new(None)),
            active_retry_runner: Arc::new(Mutex::new(None)),
            active_cancel: Arc::new(Mutex::new(None)),
            pending_save_setup: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_cancel_token(&self, token: CancellationToken) {
        *self.active_cancel.lock().unwrap() = Some(token);
    }

    pub fn cancel_active(&self) {
        if let Some(token) = self.active_cancel.lock().unwrap().take() {
            token.cancel();
        }
        self.cancel_save_setup();
        self.cancel_conflict();
    }

    pub fn set_conflict_resolver(&self, resolver: ConflictResolver) {
        *self.active_conflict_resolver.lock().unwrap() = Some(resolver);
    }

    pub fn on_conflict(&self, conflict: SaveConflict) {
        *self.current_conflict.lock().unwrap() = Some(conflict);
    }

    pub fn resolve_conflict(&self, decision: Decision) -> bool {
        let (conflict, resolver) = {
            let mut conf_guard = self.current_conflict.lock().unwrap();
            let conflict = conf_guard.take();
            let res_guard = self.active_conflict_resolver.lock().unwrap();
            let resolver = res_guard.clone();
            (conflict, resolver)
        };

        if let (Some(conflict), Some(resolver)) = (conflict, resolver) {
            resolver(&conflict, decision);
            (self.redraw)();
            true
        } else {
            false
        }
    }

    pub fn cancel_conflict(&self) {
        let (conflict, resolver) = {
            let mut conf_guard = self.current_conflict.lock().unwrap();
            let conflict = conf_guard.take();
            let res_guard = self.active_conflict_resolver.lock().unwrap();
            let resolver = res_guard.clone();
            (conflict, resolver)
        };
        if let (Some(conflict), Some(resolver)) = (conflict, resolver) {
            resolver(&conflict, Decision::Conflict);
        }
    }

    pub fn set_retry(&self, failure: PostSyncFailure, runner: RetryRunner) {
        *self.active_retry_failure.lock().unwrap() = Some(failure);
        *self.active_retry_runner.lock().unwrap() = Some(runner);
    }

    pub fn has_retry(&self) -> bool {
        self.active_retry_runner.lock().unwrap().is_some()
    }

    pub fn take_retry(&self) -> Option<(PostSyncFailure, RetryRunner)> {
        let failure = self.active_retry_failure.lock().unwrap().take()?;
        let runner = self.active_retry_runner.lock().unwrap().take()?;
        Some((failure, runner))
    }

    pub fn clear_controls(&self) {
        *self.current_conflict.lock().unwrap() = None;
        *self.active_conflict_resolver.lock().unwrap() = None;
        *self.active_retry_failure.lock().unwrap() = None;
        *self.active_retry_runner.lock().unwrap() = None;
        *self.active_cancel.lock().unwrap() = None;
        self.cancel_save_setup();
    }

    pub fn cancel_save_setup(&self) {
        if let Some((_, sender)) = self.pending_save_setup.lock().unwrap().take() {
            let _ = sender.send(Err("Save setup cancelled.".into()));
        }
    }

    /// Cancels the pending request only if it is the one for `game_name`, so a late
    /// cancel of a dismissed dialog cannot cancel a newer request.
    pub fn cancel_save_setup_for(&self, game_name: &str) {
        let mut pending = self.pending_save_setup.lock().unwrap();
        if pending.as_ref().is_some_and(|(name, _)| name == game_name)
            && let Some((_, sender)) = pending.take()
        {
            let _ = sender.send(Err("Save setup cancelled.".into()));
        }
    }

    pub fn complete_save_setup_response(
        &self,
        game_name: &str,
        game: SyncGame,
        targets: Vec<emusync_domain::library::SaveTarget>,
    ) -> bool {
        let mut pending = self.pending_save_setup.lock().unwrap();
        if let Some((name, _)) = pending.as_ref()
            && name == game_name
        {
            let (_, sender) = pending.take().unwrap();
            let _ = sender.send(Ok(SaveSetupResult { game, targets }));
            (self.redraw)();
            return true;
        }
        false
    }
}

pub struct AsyncSaveSetupBridge {
    pub controls: InteractionControls,
}

impl SaveSetup for AsyncSaveSetupBridge {
    type Error = String;

    async fn request(&self, request: SaveSetupRequest) -> Result<SaveSetupResult, Self::Error> {
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.controls.pending_save_setup.lock().unwrap();
            *pending = Some((request.game.name.clone(), tx));
        }

        let item = GameItem::new(
            request.game.name.clone(),
            request.game.entry.clone(),
            request.game.rom_path.as_ref().map(PathBuf::from),
        );
        // Prefill with the game's own targets, never the emulator-wide fallbacks.
        let entry = item.entry.clone();
        let rom_name = item.rom_file_name();
        let targets = tokio::task::spawn_blocking(move || {
            game_specific_targets(&LocalSaveTargets, &entry, rom_name.as_deref())
        })
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
        let ui_request = emusync_presentation::model::SaveSetupRequest {
            item,
            notice: None,
            cloud_file_names: request.cloud_file_names,
            targets,
        };

        let _ = self.controls.event_tx.send(AppEvent::SaveSetupRequested {
            request: Box::new(ui_request),
        });
        (self.controls.redraw)();

        rx.await
            .map_err(|_| "Save setup was cancelled or closed.".to_string())?
    }
}
