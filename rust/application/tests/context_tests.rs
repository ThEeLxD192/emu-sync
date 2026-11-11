use emusync_application::{SyncContext, SyncServices};
use emusync_domain::config::{AppConfig, CloudAuthorization};
use emusync_domain::execution::{GameLaunchRequest, GameLauncher, GameResult};
use emusync_domain::library::{Emulator, GameEntry, SaveTarget};
use emusync_domain::sync::{
    CloudFileListing, CloudFolderStore, Decision, LocalSaveAccess, LocalSaveFiles, LocalSavePath,
    PostSyncFailure, RemoteFile, RemoteState, RequestId, SaveConflict, SyncEvent,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct DummyPath(String);

impl LocalSavePath for DummyPath {
    type Error = String;
    fn path(&self) -> &str {
        &self.0
    }
    fn name(&self) -> &str {
        &self.0
    }
    fn absolute_path(&self) -> Result<String, Self::Error> {
        Ok(self.0.clone())
    }
    fn normalized_path(&self) -> Result<String, Self::Error> {
        Ok(self.0.clone())
    }
    fn ancestors(&self) -> Result<Vec<String>, Self::Error> {
        Ok(Vec::new())
    }
    fn is_file(&self) -> Result<bool, Self::Error> {
        Ok(true)
    }
    fn is_directory(&self) -> Result<bool, Self::Error> {
        Ok(false)
    }
    fn exists(&self) -> Result<bool, Self::Error> {
        Ok(true)
    }
    fn last_modified_ms(&self) -> Result<i64, Self::Error> {
        Ok(0)
    }
    fn resolve(&self, relative: &str) -> Result<Self, Self::Error> {
        Ok(DummyPath(format!("{}/{}", self.0, relative)))
    }
    fn resolve_remote(&self, name: &str) -> Result<Option<Self>, Self::Error> {
        Ok(Some(DummyPath(format!("{}/{}", self.0, name))))
    }
    fn walk_top_down(&self) -> Result<Vec<Self>, Self::Error> {
        Ok(Vec::new())
    }
    fn relative_to(&self, _root: &Self) -> Result<String, Self::Error> {
        Ok(String::new())
    }
}

#[derive(Clone, Default)]
struct DummyAccess;

impl LocalSaveAccess for DummyAccess {
    type Error = String;
    type Path = DummyPath;

    fn path(&self, path: &str) -> Self::Path {
        DummyPath(path.to_string())
    }
    fn effective_targets(&self, entry: &GameEntry) -> Result<Vec<SaveTarget>, Self::Error> {
        Ok(entry.common().save_targets.clone())
    }
    fn effective_targets_for_rom(
        &self,
        emu: &Emulator,
        _rom: &str,
    ) -> Result<Vec<SaveTarget>, Self::Error> {
        Ok(emu.common.save_targets.clone())
    }
}

#[derive(Clone, Default)]
struct DummyListing;

impl CloudFileListing for DummyListing {
    type Error = String;
    async fn list_files(
        &self,
        _token: &str,
        _folder: &str,
    ) -> Result<Vec<RemoteFile>, Self::Error> {
        Ok(Vec::new())
    }
    async fn list_children(
        &self,
        _token: &str,
        _folder: &str,
    ) -> Result<Vec<RemoteFile>, Self::Error> {
        Ok(Vec::new())
    }
}

#[derive(Clone, Default)]
struct DummyFolders;

impl CloudFolderStore for DummyFolders {
    type Error = String;
    async fn ensure_path(&self, _token: &str, _segments: &[String]) -> Result<String, Self::Error> {
        Ok("folder_id".into())
    }
    async fn find_path(
        &self,
        _token: &str,
        _segments: &[String],
    ) -> Result<Option<String>, Self::Error> {
        Ok(Some("folder_id".into()))
    }
    async fn rename_folder(
        &self,
        _token: &str,
        _folder: &str,
        _name: &str,
        _add_parent: Option<&str>,
        _remove_parent: Option<&str>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn move_file(
        &self,
        _token: &str,
        _file: &str,
        _add_parent: &str,
        _remove_parent: &str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct DummyTracked;

impl LocalSaveFiles for DummyTracked {
    type Error = String;

    async fn plan(
        &self,
        _local: &str,
        _remote: Option<&RemoteState>,
        _folder: &str,
        _remote_name: &str,
    ) -> Result<Decision, Self::Error> {
        Ok(Decision::InSync)
    }

    async fn remember(
        &self,
        _local: &str,
        _remote: &RemoteState,
        _folder: &str,
        _remote_name: &str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn download(
        &self,
        _token: &str,
        _remote: &RemoteState,
        _local: &str,
        _folder: &str,
        _remote_name: &str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn upload(
        &self,
        _token: &str,
        _local: &str,
        _remote: Option<&RemoteState>,
        _folder: &str,
        _remote_name: &str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct DummyAuth;

impl CloudAuthorization for DummyAuth {
    type Error = String;
    async fn authorize(
        &self,
        _config: &AppConfig,
        _interactive: bool,
    ) -> Result<String, Self::Error> {
        Ok("dummy_token".into())
    }
}

#[derive(Clone, Default)]
struct DummyLauncher;

impl GameLauncher for DummyLauncher {
    type Error = String;
    async fn launch(&self, _request: GameLaunchRequest) -> Result<GameResult, Self::Error> {
        Ok(GameResult {
            exit_code: 0,
            duration_ms: 100,
        })
    }
}

fn create_context(
    on_event: impl Fn(SyncEvent) + Send + Sync + 'static,
) -> SyncContext<DummyAccess, DummyListing, DummyFolders, DummyTracked, DummyAuth, DummyLauncher> {
    let services = SyncServices {
        files: DummyAccess,
        search: DummyListing,
        folders: DummyFolders,
        tracked_sync: DummyTracked,
        authorization: DummyAuth,
        runner: DummyLauncher,
    };
    SyncContext::new(AppConfig::default(), services, on_event)
}

#[test]
fn monotonic_request_id_generation() {
    let ctx = create_context(|_| {});
    let id1 = ctx.next_request_id();
    let id2 = ctx.next_request_id();
    let id3 = ctx.next_request_id();

    assert_eq!(id1, RequestId(1));
    assert_eq!(id2, RequestId(2));
    assert_eq!(id3, RequestId(3));
}

#[tokio::test]
async fn request_conflict_resolved_by_matching_id() {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = Arc::new(Mutex::new(Some(tx)));

    let ctx = Arc::new(create_context(move |ev| {
        if let SyncEvent::Conflict(req) = ev
            && let Some(sender) = tx.lock().unwrap().take()
        {
            let _ = sender.send(req);
        }
    }));

    let ctx_clone = Arc::clone(&ctx);
    let task = tokio::spawn(async move {
        ctx_clone
            .request_conflict(1000, 2000, "Super Metroid".into())
            .await
    });

    let conflict = rx.await.expect("conflict should be emitted");
    assert_eq!(conflict.game_name, "Super Metroid");

    // Resolve conflict with UploadLocal
    ctx.resolve_conflict(&conflict, Decision::UploadLocal);

    let decision = task.await.unwrap();
    assert_eq!(decision, Decision::UploadLocal);
}

#[tokio::test]
async fn resolve_conflict_ignores_mismatched_request_id() {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = Arc::new(Mutex::new(Some(tx)));

    let ctx = Arc::new(create_context(move |ev| {
        if let SyncEvent::Conflict(req) = ev
            && let Some(sender) = tx.lock().unwrap().take()
        {
            let _ = sender.send(req);
        }
    }));

    let ctx_clone = Arc::clone(&ctx);
    let task = tokio::spawn(async move {
        ctx_clone
            .request_conflict(1000, 2000, "Chrono Trigger".into())
            .await
    });

    let conflict = rx.await.expect("conflict should be emitted");

    // Attempt resolution with a mismatched RequestId
    let fake_conflict = SaveConflict {
        request_id: RequestId(9999),
        local_modified_ms: 1000,
        cloud_modified_ms: 2000,
        game_name: "Chrono Trigger".into(),
    };
    ctx.resolve_conflict(&fake_conflict, Decision::DownloadCloud);

    // Now resolve with the correct RequestId
    ctx.resolve_conflict(&conflict, Decision::UploadLocal);

    let decision = task.await.unwrap();
    assert_eq!(decision, Decision::UploadLocal);
}

#[tokio::test]
async fn aborted_conflict_request_cleans_up_pending_state_without_clobbering_newer_conflict() {
    let (tx_events, mut rx_events) = tokio::sync::mpsc::unbounded_channel();

    let ctx = Arc::new(create_context(move |ev| {
        if let SyncEvent::Conflict(req) = ev {
            let _ = tx_events.send(req);
        }
    }));

    // 1. Launch first conflict request in a tokio task
    let ctx_1 = Arc::clone(&ctx);
    let task1 =
        tokio::spawn(async move { ctx_1.request_conflict(1000, 2000, "Task 1".into()).await });

    // 2. Wait deterministically for conflict 1 to be emitted
    let conflict1 = rx_events.recv().await.expect("conflict 1 must be emitted");
    assert_eq!(conflict1.game_name, "Task 1");

    // 3. Abort task 1 and wait for it to finish as cancelled
    task1.abort();
    let join_err = task1.await.unwrap_err();
    assert!(join_err.is_cancelled());

    // 4. Verify that resolving conflict 1 now has no effect (CleanupGuard dropped and cleared pending)
    ctx.resolve_conflict(&conflict1, Decision::UploadLocal);

    // 5. Launch second conflict request in another task
    let ctx_2 = Arc::clone(&ctx);
    let task2 =
        tokio::spawn(async move { ctx_2.request_conflict(3000, 4000, "Task 2".into()).await });

    // Verify it receives a strictly higher RequestId
    let conflict2 = rx_events.recv().await.expect("conflict 2 must be emitted");
    assert_eq!(conflict2.game_name, "Task 2");
    assert!(conflict2.request_id.0 > conflict1.request_id.0);

    // Resolving conflict 1 again does not resolve conflict 2
    ctx.resolve_conflict(&conflict1, Decision::UploadLocal);

    // Resolving conflict 2 succeeds and delivers Decision::DownloadCloud
    ctx.resolve_conflict(&conflict2, Decision::DownloadCloud);
    let decision2 = task2.await.unwrap();
    assert_eq!(decision2, Decision::DownloadCloud);
}

#[tokio::test]
async fn report_post_sync_failure_and_retry_executes_retry_future() {
    let emitted_failure = Arc::new(Mutex::new(None));
    let emitted_clone = Arc::clone(&emitted_failure);

    let ctx = create_context(move |ev| {
        if let SyncEvent::PostSyncFailed(f) = ev {
            *emitted_clone.lock().unwrap() = Some(f);
        }
    });

    let retry_invocations = Arc::new(AtomicUsize::new(0));
    let retry_invocations_clone = Arc::clone(&retry_invocations);

    let failure = PostSyncFailure {
        request_id: RequestId(42),
        game_name: "Zelda".into(),
        reason: "Network dropped".into(),
        completed_save_count: 1,
        pending_save_count: 2,
    };

    ctx.report_post_sync_failure(failure.clone(), move || {
        let invocations = Arc::clone(&retry_invocations_clone);
        async move {
            invocations.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    });

    assert_eq!(
        emitted_failure
            .lock()
            .unwrap()
            .as_ref()
            .map(|f| &f.game_name),
        Some(&"Zelda".to_string())
    );

    // Mismatched failure request_id is ignored
    let mismatched = PostSyncFailure {
        request_id: RequestId(999),
        game_name: "Zelda".into(),
        reason: "Network dropped".into(),
        completed_save_count: 1,
        pending_save_count: 2,
    };
    let res_mismatch = ctx.retry_post_sync(&mismatched).await;
    assert_eq!(res_mismatch, Ok(()));
    assert_eq!(retry_invocations.load(Ordering::SeqCst), 0);

    // Matching failure executes the retry
    let res_match = ctx.retry_post_sync(&failure).await;
    assert_eq!(res_match, Ok(()));
    assert_eq!(retry_invocations.load(Ordering::SeqCst), 1);

    // Subsequent retry when no pending retry exists returns Ok(()) without re-executing
    let res_subsequent = ctx.retry_post_sync(&failure).await;
    assert_eq!(res_subsequent, Ok(()));
    assert_eq!(retry_invocations.load(Ordering::SeqCst), 1);
}
