use std::collections::HashMap;

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

const DIRECTORY_CONFLICT_THRESHOLD_SECONDS: u64 = 5;

pub fn plan_directory(
    local: &[LocalFile],
    cloud: &[CloudFile],
    action: LocalNewerAction,
) -> Vec<PlannedFile> {
    let mut locals: HashMap<&str, &LocalFile> = HashMap::new();
    let mut clouds: HashMap<&str, Vec<&CloudFile>> = HashMap::new();
    for file in local {
        locals.insert(file.path.as_str(), file);
    }
    for file in cloud {
        clouds.entry(file.path.as_str()).or_default().push(file);
    }
    let mut paths: Vec<&str> = locals.keys().chain(clouds.keys()).copied().collect();
    paths.sort_unstable_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    paths.dedup();
    paths
        .into_iter()
        .map(|path| {
            let local = locals.get(path).map(|f| (*f).clone());
            let mut duplicates = clouds.remove(path).unwrap_or_default();
            duplicates.sort_by(|a, b| {
                a.modified
                    .cmp(&b.modified)
                    .then_with(|| a.id.encode_utf16().cmp(b.id.encode_utf16()))
            });
            let cloud = duplicates.pop().cloned();
            let decision = plan_sync(
                local.as_ref().map(|f| f.modified),
                cloud.as_ref().map(|f| f.modified),
                DIRECTORY_CONFLICT_THRESHOLD_SECONDS,
                action,
            );
            PlannedFile {
                path: path.to_owned(),
                local,
                cloud,
                duplicates: duplicates.into_iter().cloned().collect(),
                decision,
            }
        })
        .collect()
}
