//! Escritura atómica común: archivo temporal único, `sync_all` y `rename`.

use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Nombre temporal único (proceso + contador + nanos) junto a `target`.
pub(crate) fn staged_path(target: &Path, tag: &str, extension: &str) -> PathBuf {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(
        ".{name}.{tag}.{}-{seq}-{nanos}.{extension}",
        std::process::id()
    ))
}

struct StagedGuard<'a> {
    path: &'a Path,
    armed: bool,
}

impl Drop for StagedGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(self.path);
        }
    }
}

/// Escribe `bytes` en un temporal exclusivo, invoca `before_replace` y renombra sobre `target`.
/// En error el temporal se elimina. `mode` solo se aplica en Unix.
pub(crate) fn write_atomic(
    target: &Path,
    bytes: &[u8],
    mode: Option<u32>,
    before_replace: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let staged = staged_path(target, "write", "tmp");

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    if let Some(mode) = mode {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode);
    }

    let mut guard = StagedGuard {
        path: &staged,
        armed: false,
    };
    let mut file = options.open(&staged)?;
    guard.armed = true;
    file.write_all(bytes)?;
    #[cfg(unix)]
    if let Some(mode) = mode {
        use std::os::unix::fs::PermissionsExt;
        // El umask puede haber recortado el modo de creación.
        file.set_permissions(fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    file.sync_all()?;
    drop(file);

    before_replace(&staged)?;
    fs::rename(&staged, target)?;
    guard.armed = false;
    Ok(())
}
