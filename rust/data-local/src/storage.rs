use std::{
    collections::HashMap,
    fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, Weak},
};

pub(crate) fn absolute(path: &Path) -> io::Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    Ok(result)
}

pub(crate) fn lock_for(path: &Path) -> io::Result<Arc<Mutex<()>>> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();
    let mut locks = LOCKS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| io::Error::other("lock registry poisoned"))?;
    let path = absolute(path)?;
    if let Some(lock) = locks.get(&path).and_then(Weak::upgrade) {
        return Ok(lock);
    }
    locks.retain(|_, lock| lock.strong_count() > 0);
    let lock = Arc::new(Mutex::new(()));
    locks.insert(path, Arc::downgrade(&lock));
    Ok(lock)
}

pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    atomic_write_with(path, contents, || Ok(()))
}

fn atomic_write_with(
    path: &Path,
    contents: &[u8],
    before_replace: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    let parent = match path.parent() {
        Some(parent) if parent.as_os_str().is_empty() => Path::new("."),
        Some(parent) => parent,
        None => return Err(io::Error::other("missing parent directory")),
    };
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::Builder::new()
        .prefix(".emusync.")
        .suffix(".tmp")
        .tempfile_in(parent)?;
    restrict_file(temp.path())?;
    temp.write_all(contents)?;
    temp.as_file().sync_all()?;
    before_replace()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(unix)]
pub(crate) fn restrict_file(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}
#[cfg(not(unix))]
pub(crate) fn restrict_file(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
pub(crate) fn restrict_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}
#[cfg(not(unix))]
pub(crate) fn restrict_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_replacement_retains_old_bytes_and_removes_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        fs::write(&path, b"old complete state").unwrap();
        assert!(
            atomic_write_with(&path, b"new state", || Err(io::Error::other(
                "simulated failure"
            )))
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), b"old complete state");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
