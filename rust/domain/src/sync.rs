use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    UploadLocal,
    DownloadCloud,
    InSync,
    Conflict,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalNewerAction {
    ResolveConflict,
    Upload,
}

/// Nanoseconds since the Unix epoch preserve fractional remote timestamps.
pub type Timestamp = i128;

pub fn plan_sync(
    local: Option<Timestamp>,
    cloud: Option<Timestamp>,
    threshold_seconds: u64,
    action: LocalNewerAction,
) -> Decision {
    match (local, cloud) {
        (None, None) => Decision::InSync,
        (None, Some(_)) => Decision::DownloadCloud,
        (Some(_), None) => Decision::UploadLocal,
        (Some(l), Some(c))
            if action == LocalNewerAction::Upload
                && l.div_euclid(1_000_000) > c.div_euclid(1_000_000) =>
        {
            Decision::UploadLocal
        }
        (Some(l), Some(c)) if l.abs_diff(c) <= u128::from(threshold_seconds) * 1_000_000_000 => {
            Decision::InSync
        }
        (Some(l), Some(c)) if l < c => Decision::DownloadCloud,
        _ => Decision::Conflict,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteState {
    pub id: String,
    pub modified: Timestamp,
    pub sha256: Option<String>,
    pub version: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    pub local_sha256: String,
    pub remote_id: String,
    pub remote_modified: Timestamp,
    pub remote_sha256: Option<String>,
    pub remote_version: Option<String>,
}

pub fn plan_tracked(
    local: Option<&str>,
    remote: Option<&RemoteState>,
    previous: Option<&Checkpoint>,
) -> Decision {
    let (local, remote) = match (local, remote) {
        (None, None) => return Decision::InSync,
        (None, Some(_)) => return Decision::DownloadCloud,
        (Some(_), None) => return Decision::UploadLocal,
        (Some(l), Some(r)) => (l, r),
    };
    if remote
        .sha256
        .as_ref()
        .is_some_and(|h| h.eq_ignore_ascii_case(local))
    {
        return Decision::InSync;
    }
    let Some(previous) = previous else {
        return Decision::Conflict;
    };
    let remote_changed = remote.id != previous.remote_id
        || if let Some(hash) = &remote.sha256 {
            !hash.eq_ignore_ascii_case(
                previous
                    .remote_sha256
                    .as_deref()
                    .unwrap_or(&previous.local_sha256),
            )
        } else if let (Some(version), Some(old)) = (&remote.version, &previous.remote_version) {
            version != old
        } else {
            remote.modified != previous.remote_modified
        };
    match (local != previous.local_sha256, remote_changed) {
        (true, true) => Decision::Conflict,
        (true, false) => Decision::UploadLocal,
        (false, true) => Decision::DownloadCloud,
        (false, false) => Decision::InSync,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalFile {
    pub path: String,
    pub modified: Timestamp,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloudFile {
    pub id: String,
    pub path: String,
    pub modified: Timestamp,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedFile {
    pub path: String,
    pub local: Option<LocalFile>,
    pub cloud: Option<CloudFile>,
    pub duplicates: Vec<CloudFile>,
    pub decision: Decision,
}

pub fn plan_directory(
    local: &[LocalFile],
    cloud: &[CloudFile],
    action: LocalNewerAction,
) -> Vec<PlannedFile> {
    let mut locals = BTreeMap::new();
    let mut clouds: BTreeMap<&str, Vec<CloudFile>> = BTreeMap::new();
    for file in local {
        locals.insert(file.path.as_str(), file.clone());
    }
    for file in cloud {
        clouds.entry(&file.path).or_default().push(file.clone());
    }
    let mut paths: Vec<_> = locals
        .keys()
        .chain(clouds.keys())
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    paths.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    paths
        .into_iter()
        .map(|path| {
            let local = locals.get(path).cloned();
            let mut duplicates = clouds.remove(path).unwrap_or_default();
            duplicates.sort_by(|a, b| {
                a.modified
                    .cmp(&b.modified)
                    .then_with(|| a.id.encode_utf16().cmp(b.id.encode_utf16()))
            });
            let cloud = duplicates.pop();
            let decision = plan_sync(
                local.as_ref().map(|f| f.modified),
                cloud.as_ref().map(|f| f.modified),
                5,
                action,
            );
            PlannedFile {
                path: path.to_owned(),
                local,
                cloud,
                duplicates,
                decision,
            }
        })
        .collect()
}

/// Reject absolute, ambiguous and traversal names before an adapter resolves them.
pub fn remote_path_segments(name: &str) -> Option<Vec<&str>> {
    if name.trim() != name
        || name.is_empty()
        || name.contains('\0')
        || name.starts_with(['/', '\\'])
        || (name.as_bytes().get(1) == Some(&b':') && name.as_bytes()[0].is_ascii_alphabetic())
    {
        return None;
    }
    let segments: Vec<_> = name.split(['/', '\\']).collect();
    if segments
        .iter()
        .any(|s| s.trim().is_empty() || *s == "." || *s == "..")
    {
        None
    } else {
        Some(segments)
    }
}

/// A provider-independent file observation; wire timestamps are decoded by adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteFile {
    pub id: String,
    pub name: String,
    pub modified: Timestamp,
    pub is_folder: bool,
    pub sha256: Option<String>,
    pub version: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnverifiedDuplicates {
    pub name: String,
}

/// Refuses duplicates whose equal content cannot be established; no copy is discarded.
pub fn canonical_cloud_files(
    files: &[RemoteFile],
) -> Result<Vec<RemoteFile>, UnverifiedDuplicates> {
    let mut groups: Vec<Vec<&RemoteFile>> = Vec::new();
    for file in files {
        if let Some(group) = groups.iter_mut().find(|g| g[0].name == file.name) {
            group.push(file);
        } else {
            groups.push(vec![file]);
        }
    }
    groups
        .into_iter()
        .map(|group| {
            if group.len() > 1 {
                let first = group[0].sha256.as_deref();
                if first.is_none()
                    || group.iter().any(|f| {
                        !f.sha256.as_deref().is_some_and(|hash| {
                            hash.eq_ignore_ascii_case(first.unwrap_or_default())
                        })
                    })
                {
                    return Err(UnverifiedDuplicates {
                        name: group[0].name.clone(),
                    });
                }
            }
            Ok((*group
                .into_iter()
                .max_by(|a, b| {
                    a.modified
                        .cmp(&b.modified)
                        .then_with(|| a.id.encode_utf16().cmp(b.id.encode_utf16()))
                })
                .expect("nonempty group"))
            .clone())
        })
        .collect()
}

pub fn sanitize_cloud_folder_name(name: &str) -> String {
    let name = name.replace(['/', '\\'], "-");
    let name = name.trim();
    if name.is_empty() {
        "Unknown".into()
    } else {
        name.into()
    }
}
fn normalize_save_path(path: &str) -> String {
    let replaced = path.trim().replace('\\', "/");
    let trimmed = replaced.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".into()
    } else {
        trimmed.into()
    }
}
pub fn cloud_path_segments(
    entry: &crate::library::GameEntry,
    rom_name: Option<&str>,
    game_title: Option<&str>,
    global: &[crate::library::SaveTarget],
    per_rom: &[crate::library::SaveTarget],
) -> Vec<String> {
    use crate::library::GameEntry;
    let common = entry.common();
    let mut segments = vec![common.effective_cloud_folder().to_owned()];
    match entry {
        GameEntry::Emulator(emulator) => {
            let global_paths: BTreeSet<_> = global
                .iter()
                .map(|t| (normalize_save_path(&t.path), &t.cloud_path))
                .collect();
            let distinct = per_rom
                .iter()
                .any(|t| !global_paths.contains(&(normalize_save_path(&t.path), &t.cloud_path)));
            if let Some(rom) = rom_name.filter(|_| distinct || global_paths.is_empty()) {
                let title = game_title
                    .map(str::to_owned)
                    .unwrap_or_else(|| emulator.effective_title(rom, rom));
                segments.push(sanitize_cloud_folder_name(&title));
            }
        }
        GameEntry::Native(_) if common.group.is_some() && common.cloud_folder.is_none() => {
            segments.push(sanitize_cloud_folder_name(&common.name))
        }
        GameEntry::Native(_) => {}
    }
    segments
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncGame {
    pub name: String,
    pub entry: crate::library::GameEntry,
    pub rom_path: Option<String>,
    pub effective_title: String,
    pub save_targets: Vec<crate::library::SaveTarget>,
}
/// Unique within the synchronizer lifetime, even when requests have equal contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RequestId(pub u64);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveConflict {
    pub request_id: RequestId,
    pub local_modified_ms: i64,
    pub cloud_modified_ms: i64,
    pub game_name: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostSyncFailure {
    pub request_id: RequestId,
    pub game_name: String,
    pub reason: String,
    pub completed_save_count: usize,
    pub pending_save_count: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncEvent {
    Idle,
    Syncing(String),
    Playing,
    Error(String),
    Conflict(SaveConflict),
    PostSyncFailed(PostSyncFailure),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntrySyncStatus {
    Idle,
    InSync,
    OutOfSync,
    Conflict,
    NotConfigured,
    Error,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveSetupRequest {
    pub game: SyncGame,
    pub cloud_file_names: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveSetupResult {
    pub game: SyncGame,
    pub targets: Vec<crate::library::SaveTarget>,
}

/// Cancellation is cooperative: implementations finish cleanup and remote rollback before returning.
pub trait CloudSaveTransfer {
    type Error;
    fn download(
        &self,
        token: &str,
        file_id: &str,
        destination: &str,
        modified: Option<Timestamp>,
        progress_name: &str,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
    fn upload(
        &self,
        token: &str,
        source: &str,
        remote_name: &str,
        parent: &str,
    ) -> impl std::future::Future<Output = Result<RemoteState, Self::Error>> + Send;
    /// Replace only the observed revision; precondition failure must not fall back to unconditional upload.
    fn replace(
        &self,
        token: &str,
        observed: &RemoteState,
        source: &str,
        remote_name: &str,
    ) -> impl std::future::Future<Output = Result<RemoteState, Self::Error>> + Send;
}
pub trait CloudFileListing {
    type Error;
    fn list_files(
        &self,
        token: &str,
        folder: &str,
    ) -> impl std::future::Future<Output = Result<Vec<RemoteFile>, Self::Error>> + Send;
    fn list_children(
        &self,
        token: &str,
        folder: &str,
    ) -> impl std::future::Future<Output = Result<Vec<RemoteFile>, Self::Error>> + Send;
}
pub trait CloudFolderStore {
    type Error;
    fn ensure_path(
        &self,
        token: &str,
        segments: &[String],
    ) -> impl std::future::Future<Output = Result<String, Self::Error>> + Send;
    fn find_path(
        &self,
        token: &str,
        segments: &[String],
    ) -> impl std::future::Future<Output = Result<Option<String>, Self::Error>> + Send;
    fn rename_folder(
        &self,
        token: &str,
        folder: &str,
        name: &str,
        add_parent: Option<&str>,
        remove_parent: Option<&str>,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
    fn move_file(
        &self,
        token: &str,
        file: &str,
        add_parent: &str,
        remove_parent: &str,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
}
/// Filesystem observations and safe resolution are supplied by a local adapter.
pub trait LocalSavePath: Sized {
    type Error;
    fn path(&self) -> &str;
    fn name(&self) -> &str;
    fn absolute_path(&self) -> Result<String, Self::Error>;
    fn normalized_path(&self) -> Result<String, Self::Error>;
    fn ancestors(&self) -> Result<Vec<String>, Self::Error>;
    fn is_file(&self) -> Result<bool, Self::Error>;
    fn is_directory(&self) -> Result<bool, Self::Error>;
    fn exists(&self) -> Result<bool, Self::Error>;
    fn last_modified_ms(&self) -> Result<i64, Self::Error>;
    fn resolve(&self, relative: &str) -> Result<Self, Self::Error>;
    fn resolve_remote(&self, name: &str) -> Result<Option<Self>, Self::Error>;
    fn walk_top_down(&self) -> Result<Vec<Self>, Self::Error>;
    /// Always uses '/' separators.
    fn relative_to(&self, root: &Self) -> Result<String, Self::Error>;
}
pub trait LocalSaveAccess {
    type Error;
    type Path: LocalSavePath;
    fn path(&self, path: &str) -> Self::Path;
    fn effective_targets(
        &self,
        entry: &crate::library::GameEntry,
    ) -> Result<Vec<crate::library::SaveTarget>, Self::Error>;
    fn effective_targets_for_rom(
        &self,
        entry: &crate::library::Emulator,
        rom: &str,
    ) -> Result<Vec<crate::library::SaveTarget>, Self::Error>;
}
/// Local history, snapshots and backup handling around transfers.
pub trait LocalSaveFiles {
    type Error;
    fn plan(
        &self,
        local: &str,
        remote: Option<&RemoteState>,
        folder: &str,
        remote_name: &str,
    ) -> impl std::future::Future<Output = Result<Decision, Self::Error>> + Send;
    fn remember(
        &self,
        local: &str,
        remote: &RemoteState,
        folder: &str,
        remote_name: &str,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
    fn download(
        &self,
        token: &str,
        remote: &RemoteState,
        local: &str,
        folder: &str,
        remote_name: &str,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
    fn upload(
        &self,
        token: &str,
        local: &str,
        remote: Option<&RemoteState>,
        folder: &str,
        remote_name: &str,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
}
pub trait SaveSetup {
    type Error;
    fn request(
        &self,
        request: SaveSetupRequest,
    ) -> impl std::future::Future<Output = Result<SaveSetupResult, Self::Error>> + Send;
}
/// The owner must not drop a running operation on cancellation; it must await its cleanup.
pub trait SaveSynchronizer {
    type Error;
    fn check_entry(
        &self,
        entry: &crate::library::GameEntry,
    ) -> impl std::future::Future<Output = Result<EntrySyncStatus, Self::Error>> + Send;
    fn sync_entry(
        &self,
        entry: &crate::library::GameEntry,
    ) -> impl std::future::Future<Output = Result<bool, Self::Error>> + Send;
    fn rename_game(
        &self,
        entry: &crate::library::GameEntry,
        rom: Option<&str>,
        old_title: &str,
        new_title: &str,
        targets: &[crate::library::SaveTarget],
    ) -> impl std::future::Future<Output = Result<bool, Self::Error>> + Send;
    fn rename_entry(
        &self,
        old: &crate::library::GameEntry,
        new: &crate::library::GameEntry,
    ) -> impl std::future::Future<Output = Result<bool, Self::Error>> + Send;
    fn play_with_sync(
        &self,
        game: SyncGame,
        setup: &(impl SaveSetup + Sync),
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
    /// Only the exact pending request is resolved; completed requests are ignored.
    fn resolve_conflict(&self, request: &SaveConflict, decision: Decision);
    /// A stale failure must not replay a newer batch.
    fn retry_post_sync(
        &self,
        failure: &PostSyncFailure,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
}

pub trait SaveSynchronizerFactory {
    type Synchronizer: SaveSynchronizer;
    fn create(
        &self,
        config: crate::config::AppConfig,
        on_status: impl Fn(SyncEvent) + Send + Sync + 'static,
    ) -> Self::Synchronizer;
}
