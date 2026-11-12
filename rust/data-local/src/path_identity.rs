use crate::storage::absolute;
use emusync_domain::sync::{local_names_ignore_case, name_key};
use std::{
    ffi::{OsStr, OsString},
    io,
    path::{Path, PathBuf},
};

/// A path normalised so that two spellings of the same file compare equal.
///
/// It is absolute and lexically clean, and with `ignore_case` every component is folded the
/// way NTFS compares names, so `C:\Saves` and `c:\saves` yield one identity. Use it only as
/// a comparison or registry key: it may not spell the file as it is on disk.
pub fn path_identity(path: &Path, ignore_case: bool) -> io::Result<PathBuf> {
    let mut identity = PathBuf::new();
    for part in absolute(path)?.components() {
        identity.push(fold(part.as_os_str(), ignore_case));
    }
    Ok(identity)
}

/// Whether two paths denote the same file, without touching the file system.
pub fn same_path(a: &Path, b: &Path, ignore_case: bool) -> io::Result<bool> {
    Ok(path_identity(a, ignore_case)? == path_identity(b, ignore_case)?)
}

/// `path` relative to `root` when `root` is a leading whole-component prefix of it, ignoring
/// letter case if asked. The result keeps `path`'s own spelling.
pub fn relative_within(path: &Path, root: &Path, ignore_case: bool) -> Option<PathBuf> {
    let mut rest = path.components();
    for expected in root.components() {
        let actual = rest.next()?;
        if fold(actual.as_os_str(), ignore_case) != fold(expected.as_os_str(), ignore_case) {
            return None;
        }
    }
    Some(rest.as_path().to_owned())
}

/// [`path_identity`] under this platform's case rule.
pub(crate) fn local_identity(path: &Path) -> io::Result<PathBuf> {
    path_identity(path, local_names_ignore_case())
}

fn fold(part: &OsStr, ignore_case: bool) -> OsString {
    match part.to_str() {
        Some(text) if ignore_case => name_key(text, true).into_owned().into(),
        // A name that is not valid Unicode has no defined case; compare it exactly.
        _ => part.to_owned(),
    }
}
