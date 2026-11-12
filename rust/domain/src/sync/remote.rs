use super::decision::Timestamp;
use super::name_case::{local_names_ignore_case, name_key};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;

/// Reject absolute, ambiguous and traversal names before an adapter resolves them.
pub fn remote_path_segments(name: &str) -> Option<Vec<&str>> {
    if name.trim() != name
        || name.is_empty()
        || name.contains('\0')
        || name.starts_with(['/', '\\'])
        || matches!(name.as_bytes(), [drive, b':', ..] if drive.is_ascii_alphabetic())
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

impl fmt::Display for UnverifiedDuplicates {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "several cloud copies of '{}' contain different or unverified progress",
            self.name
        )
    }
}

impl std::error::Error for UnverifiedDuplicates {}

/// Refuses duplicates whose equal content cannot be established; no copy is discarded.
///
/// Names that differ only in letter case are duplicates on file systems that ignore case
/// (Windows), because both would be written to the same local file.
pub fn canonical_cloud_files(
    files: &[RemoteFile],
) -> Result<Vec<RemoteFile>, UnverifiedDuplicates> {
    canonical_cloud_files_with(files, local_names_ignore_case())
}

/// [`canonical_cloud_files`] with the local file system's case rule given explicitly.
pub fn canonical_cloud_files_with(
    files: &[RemoteFile],
    ignore_case: bool,
) -> Result<Vec<RemoteFile>, UnverifiedDuplicates> {
    let mut index: HashMap<Cow<'_, str>, usize> = HashMap::new();
    let mut groups: Vec<(&RemoteFile, Vec<&RemoteFile>)> = Vec::new();
    for file in files {
        let key = name_key(&file.name, ignore_case);
        if let Some(&idx) = index.get(&key) {
            if let Some((_, rest)) = groups.get_mut(idx) {
                rest.push(file);
            }
        } else {
            index.insert(key, groups.len());
            groups.push((file, Vec::new()));
        }
    }
    groups
        .into_iter()
        .map(|(first, rest)| {
            if !rest.is_empty() {
                let unverified = || UnverifiedDuplicates {
                    name: first.name.clone(),
                };
                let Some(hash) = first.sha256.as_deref() else {
                    return Err(unverified());
                };
                if rest.iter().any(|f| {
                    !f.sha256
                        .as_deref()
                        .is_some_and(|h| h.eq_ignore_ascii_case(hash))
                }) {
                    return Err(unverified());
                }
            }
            let best = rest.iter().fold(first, |best, f| {
                let order = f
                    .modified
                    .cmp(&best.modified)
                    .then_with(|| f.id.encode_utf16().cmp(best.id.encode_utf16()));
                if order.is_ge() { f } else { best }
            });
            Ok(best.clone())
        })
        .collect()
}
