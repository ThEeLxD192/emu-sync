use crate::events::AppEvent;
use emusync_application::library::game_specific_targets;
use emusync_data_local::LocalSaveTargets;
use emusync_domain::sync::{
    Decision, PostSyncFailure, SaveConflict, SaveSetup, SaveSetupRequest, SaveSetupResult,
};
use emusync_presentation::model::GameItem;
use std::{
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::Sender,
    },
};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

const SAVE_SETUP_CANCELLED: &str = "Save setup cancelled.";

pub type ConflictResolver = Arc<dyn Fn(&SaveConflict, Decision) + Send + Sync>;
pub type RetryRunner = Arc<
    dyn Fn(PostSyncFailure) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync + 'static,
>;

/// A save setup request waiting for the UI. `id` tells apart two requests for the
/// same game, such as the one before and the one after playing.
pub struct PendingSaveSetup {
    pub id: u64,
    pub sender: oneshot::Sender<Result<SaveSetupResult, String>>,
}

impl PendingSaveSetup {
    /// Answers the waiting operation with a cancellation.
    pub fn cancel(self) {
        let _ = self.sender.send(Err(SAVE_SETUP_CANCELLED.into()));
    }

    pub fn complete(self, result: SaveSetupResult) {
        let _ = self.sender.send(Ok(result));
    }
}

type PendingSaveSetupSlot = Arc<Mutex<Option<PendingSaveSetup>>>;

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
    next_save_setup_id: Arc<AtomicU64>,
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
            next_save_setup_id: Arc::new(AtomicU64::new(0)),
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

    /// Takes the pending request if `filter` accepts it. The lock is released before the
    /// UI is told, and every request that leaves the slot emits one `SaveSetupClosed`.
    fn take_pending(
        &self,
        filter: impl FnOnce(&PendingSaveSetup) -> bool,
    ) -> Option<PendingSaveSetup> {
        let taken = {
            let mut slot = self.pending_save_setup.lock().unwrap();
            if slot.as_ref().is_some_and(filter) {
                slot.take()
            } else {
                None
            }
        }?;
        self.notify_closed(taken.id);
        Some(taken)
    }

    fn notify_closed(&self, id: u64) {
        let _ = self
            .event_tx
            .send(AppEvent::SaveSetupClosed { request_id: id });
        (self.redraw)();
    }

    /// Cancels whatever request is pending.
    pub fn cancel_save_setup(&self) {
        if let Some(pending) = self.take_pending(|_| true) {
            pending.cancel();
        }
    }

    /// Cancels the pending request only if it is `id`, so a late cancel of a dismissed
    /// dialog cannot cancel a newer request.
    pub fn cancel_save_setup_for(&self, id: u64) {
        if let Some(pending) = self.take_pending(|p| p.id == id) {
            pending.cancel();
        }
    }

    /// Claims request `id` so the caller answers it; nobody else can cancel it afterwards.
    pub fn take_save_setup(&self, id: u64) -> Option<PendingSaveSetup> {
        self.take_pending(|p| p.id == id)
    }
}

pub struct AsyncSaveSetupBridge {
    pub controls: InteractionControls,
}

impl SaveSetup for AsyncSaveSetupBridge {
    type Error = String;

    async fn request(&self, request: SaveSetupRequest) -> Result<SaveSetupResult, Self::Error> {
        let (tx, rx) = oneshot::channel();
        let id = self
            .controls
            .next_save_setup_id
            .fetch_add(1, Ordering::Relaxed);
        // Replace in one step so no request can slip in between; a superseded one is
        // answered and its dialog closed once the lock is released.
        let superseded = {
            let mut slot = self.controls.pending_save_setup.lock().unwrap();
            slot.replace(PendingSaveSetup { id, sender: tx })
        };
        if let Some(old) = superseded {
            self.controls.notify_closed(old.id);
            old.cancel();
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
            id,
            item,
            notice: None,
            cloud_file_names: request.cloud_file_names,
            targets,
        };

        {
            // The check and the send share one lock so `SaveSetupClosed`, which is only
            // sent after taking the slot, can never overtake `SaveSetupRequested`.
            // `Sender::send` does not block.
            let slot = self.controls.pending_save_setup.lock().unwrap();
            if !slot.as_ref().is_some_and(|p| p.id == id) {
                return Err(SAVE_SETUP_CANCELLED.to_string());
            }
            let _ = self.controls.event_tx.send(AppEvent::SaveSetupRequested {
                request: Box::new(ui_request),
            });
        }
        (self.controls.redraw)();

        rx.await
            .map_err(|_| "Save setup was cancelled or closed.".to_string())?
    }
}
