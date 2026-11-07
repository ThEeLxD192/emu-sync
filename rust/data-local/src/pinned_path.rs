//! Linux capability for retaining a validated directory across filesystem changes.
use emusync_domain::sync::remote_path_segments;
use rustix::fs::{Mode, OFlags, mkdirat, open, openat};
use std::{
    fs::File,
    io,
    os::{fd::AsRawFd, unix::fs::MetadataExt},
    path::{Path, PathBuf},
};

/// Holds the parent directory itself, rather than resolving it again by pathname.
/// This is a building block; callers still need backup, staging and atomic publication.
pub struct PinnedRemoteDestination {
    parent: File,
    leaf: String,
    logical: PathBuf,
}
impl PinnedRemoteDestination {
    /// Pin an existing selected root and existing intermediate directories. The selected
    /// root may itself be a user-selected symlink; untrusted descendant symlinks are refused.
    pub fn open(root: &Path, remote_name: &str) -> io::Result<Option<Self>> {
        Self::open_inner(root, remote_name, false)
    }
    /// Create the selected root if absent, then create descendants without following links.
    pub fn open_or_create(root: &Path, remote_name: &str) -> io::Result<Option<Self>> {
        Self::open_inner(root, remote_name, true)
    }
    fn open_inner(root: &Path, remote_name: &str, create: bool) -> io::Result<Option<Self>> {
        let Some(segments) = remote_path_segments(remote_name) else {
            return Ok(None);
        };
        if segments
            .iter()
            .any(|segment| !crate::restore_paths::is_normal_component(segment))
        {
            return Ok(None);
        }
        if create {
            // The root is selected by the user; descendants supplied by Drive are
            // handled only relative to its opened directory descriptor below.
            std::fs::create_dir_all(root)?;
        }
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC;
        let mut parent = File::from(open(root, flags, Mode::empty())?);
        for segment in &segments[..segments.len() - 1] {
            let next = openat(&parent, *segment, flags | OFlags::NOFOLLOW, Mode::empty());
            let next = match next {
                Ok(next) => next,
                Err(error) if create && error == rustix::io::Errno::NOENT => {
                    match mkdirat(&parent, *segment, Mode::from_raw_mode(0o777)) {
                        Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                        Err(error) => return Err(error.into()),
                    }
                    // Reopen without following a link, including if another process won the race.
                    openat(&parent, *segment, flags | OFlags::NOFOLLOW, Mode::empty())?
                }
                Err(error) => return Err(error.into()),
            };
            parent = File::from(next);
        }
        Ok(Some(Self {
            parent,
            leaf: segments[segments.len() - 1].to_owned(),
            logical: segments
                .iter()
                .fold(root.to_path_buf(), |path, segment| path.join(segment)),
        }))
    }
    pub(crate) fn parent_path(&self) -> PathBuf {
        PathBuf::from(format!("/proc/self/fd/{}", self.parent.as_raw_fd()))
    }
    pub(crate) fn destination_path(&self) -> PathBuf {
        self.parent_path().join(&self.leaf)
    }
    pub(crate) fn logical_path(&self) -> &Path {
        &self.logical
    }
    pub(crate) fn ensure_current(&self) -> io::Result<()> {
        let pinned = self.parent.metadata()?;
        let parent = self
            .logical
            .parent()
            .ok_or_else(|| io::Error::other("save has no parent"))?;
        let current = std::fs::metadata(parent)?;
        if pinned.dev() != current.dev() || pinned.ino() != current.ino() {
            return Err(io::Error::other("save directory changed during restore"));
        }
        Ok(())
    }
    pub(crate) fn open_existing(&self) -> io::Result<Option<File>> {
        match openat(
            &self.parent,
            self.leaf.as_str(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => {
                let file = File::from(fd);
                if !file.metadata()?.is_file() {
                    return Err(io::Error::other("save is not a regular file"));
                }
                Ok(Some(file))
            }
            Err(error) if error == rustix::io::Errno::NOENT => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    /// Exclusively create a previously absent file beneath the pinned parent.
    /// Never replaces an existing save or follows a final symlink.
    pub fn create_new(&self) -> io::Result<File> {
        Ok(File::from(openat(
            &self.parent,
            self.leaf.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )?))
    }
}
