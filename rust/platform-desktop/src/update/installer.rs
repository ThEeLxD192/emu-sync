use crate::util::atomic::staged_path;
use std::{
    fs::{self, OpenOptions},
    io,
    path::Path,
};

pub type FileOpFn = Box<dyn Fn(&Path, &Path) -> io::Result<()> + Send + Sync>;

pub struct AppImageInstaller {
    copy_fn: FileOpFn,
    move_fn: FileOpFn,
}

impl Default for AppImageInstaller {
    fn default() -> Self {
        Self {
            copy_fn: Box::new(|src, dst| {
                fs::copy(src, dst)?;
                Ok(())
            }),
            move_fn: Box::new(move_replacing),
        }
    }
}

pub fn move_replacing(src: &Path, dst: &Path) -> io::Result<()> {
    fs::rename(src, dst)
}

impl AppImageInstaller {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_hooks(
        copy_fn: impl Fn(&Path, &Path) -> io::Result<()> + Send + Sync + 'static,
        move_fn: impl Fn(&Path, &Path) -> io::Result<()> + Send + Sync + 'static,
    ) -> Self {
        Self {
            copy_fn: Box::new(copy_fn),
            move_fn: Box::new(move_fn),
        }
    }

    pub fn replace<F>(&self, source: &Path, target: &Path, on_installed: F) -> io::Result<()>
    where
        F: FnOnce() -> io::Result<()>,
    {
        if !source.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "The downloaded AppImage is not a regular file: {}",
                    source.display()
                ),
            ));
        }
        if !target.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "The current AppImage is not a regular file: {}",
                    target.display()
                ),
            ));
        }

        let source_canon = fs::canonicalize(source)?;
        let target_canon = fs::canonicalize(target)?;
        if source_canon == target_canon {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "The update must be a separate file",
            ));
        }

        let staged = staged_path(target, "update", "tmp");
        let backup = staged_path(target, "old", "bak");

        let mut staged_exists = false;
        let mut original_backed_up = false;

        let result = (|| -> io::Result<()> {
            (self.copy_fn)(source, &staged)?;
            staged_exists = true;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                // Un AppImage sin bit de ejecución no debe instalarse nunca.
                fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))?;
            }

            let file = OpenOptions::new().write(true).open(&staged)?;
            file.sync_all()?;
            drop(file);

            (self.move_fn)(target, &backup)?;
            original_backed_up = true;

            (self.move_fn)(&staged, target)?;
            staged_exists = false;

            on_installed()
        })();

        if let Err(err) = result {
            if original_backed_up {
                if let Err(restore_err) = (self.move_fn)(&backup, target) {
                    eprintln!(
                        "EmuSync: Failed to restore backup from {}: {restore_err}",
                        backup.display()
                    );
                    // Leave backup file for recovery!
                } else {
                    let _ = fs::remove_file(&backup);
                }
            }
            if staged_exists {
                let _ = fs::remove_file(&staged);
            }
            return Err(err);
        }

        if staged_exists {
            let _ = fs::remove_file(&staged);
        }
        let _ = fs::remove_file(source);
        if original_backed_up {
            let _ = fs::remove_file(&backup);
        }

        Ok(())
    }
}
