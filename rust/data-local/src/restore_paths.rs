use emusync_domain::sync::remote_path_segments;
use std::io;
use std::path::{Component, Path, PathBuf};

/// Resolve an untrusted cloud name beneath the selected folder, preserving the
/// folder's spelling (including Windows short names). This is a validation step;
/// callers must prevent filesystem changes between validation and a later write.
pub fn resolve_remote_file(root: &Path, name: &str) -> io::Result<Option<PathBuf>> {
    let Some(segments) = remote_path_segments(name) else {
        return Ok(None);
    };
    let absolute = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()?.join(root)
    };
    let root = normalize(&absolute);
    let mut resolved = root.clone();
    for segment in segments {
        // A Windows component can contain a drive prefix even within a name.
        if !is_normal_component(segment) {
            return Ok(None);
        }
        resolved.push(segment);
        match std::fs::symlink_metadata(&resolved) {
            Ok(metadata) if metadata.file_type().is_symlink() => return Ok(None),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    let canonical_root = match std::fs::symlink_metadata(&root) {
        Ok(_) => std::fs::canonicalize(&root)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => root.clone(),
        Err(error) => return Err(error),
    };
    if let Some(parent) = resolved.parent() {
        match std::fs::canonicalize(parent) {
            Ok(canonical_parent) if !canonical_parent.starts_with(canonical_root) => {
                return Ok(None);
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(Some(resolved))
}

fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

pub(crate) fn is_normal_component(segment: &str) -> bool {
    let mut components = Path::new(segment).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}
