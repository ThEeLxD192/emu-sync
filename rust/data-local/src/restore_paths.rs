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
    let root = crate::storage::absolute(root)?;
    let mut resolved = root.clone();
    for (index, segment) in segments.iter().enumerate() {
        // A Windows component can contain a drive prefix even within a name.
        if !is_normal_component(segment) {
            return Ok(None);
        }
        resolved.push(segment);
        match std::fs::symlink_metadata(&resolved) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Ok(None);
                }
                if index + 1 < segments.len() && !metadata.file_type().is_dir() {
                    return Ok(None);
                }
            }
            Err(error)
                if error.kind() == io::ErrorKind::NotFound
                    || error.kind() == io::ErrorKind::NotADirectory => {}
            Err(error) => return Err(error),
        }
    }
    let (canonical_root, check_target) = if std::fs::metadata(&root).is_ok() {
        (std::fs::canonicalize(&root)?, resolved.as_path())
    } else {
        let Some(existing_root) = first_existing_ancestor(&root)? else {
            return Ok(None);
        };
        let canonical_root = std::fs::canonicalize(&existing_root)?;
        let target = resolved.parent().unwrap_or(&resolved);
        (canonical_root, target)
    };
    let Some(existing_target) = first_existing_ancestor(check_target)? else {
        return Ok(None);
    };
    let canonical_target = std::fs::canonicalize(&existing_target)?;
    if !canonical_target.starts_with(&canonical_root) {
        return Ok(None);
    }
    Ok(Some(resolved))
}

fn first_existing_ancestor(path: &Path) -> io::Result<Option<PathBuf>> {
    for ancestor in path.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(meta) => {
                if meta.file_type().is_symlink() && std::fs::metadata(ancestor).is_err() {
                    continue;
                }
                return Ok(Some(ancestor.to_path_buf()));
            }
            Err(e)
                if e.kind() == io::ErrorKind::NotFound
                    || e.kind() == io::ErrorKind::NotADirectory =>
            {
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(None)
}

pub(crate) fn is_normal_component(segment: &str) -> bool {
    let mut components = Path::new(segment).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}
