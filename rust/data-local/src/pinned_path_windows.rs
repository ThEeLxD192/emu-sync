//! Windows capability for retaining a validated directory across filesystem changes.
//!
//! Windows has no `openat`, so the destination is confined differently: every directory from
//! the selected root down to the save's parent is opened and kept open without
//! `FILE_SHARE_DELETE`. While those handles live, no process can rename, replace or remove any
//! of those directories (or an ancestor of them), so the pathname keeps resolving to the
//! directories that were validated. Directories below the root are opened with
//! `FILE_FLAG_OPEN_REPARSE_POINT` and refused when they are links, junctions or mount points.
use emusync_domain::sync::{is_windows_file_name, remote_path_segments};
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

const FILE_SHARE_READ: u32 = 0x0000_0001;
const FILE_SHARE_WRITE: u32 = 0x0000_0002;
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

/// Holds the parent directory chain itself, rather than resolving it again by pathname.
/// This is a building block; callers still need backup, staging and atomic publication.
pub struct PinnedRemoteDestination {
    /// Open handles keep the validated directories from being renamed or removed.
    _held: Vec<File>,
    parent: PathBuf,
    leaf: String,
    logical: PathBuf,
    depth: usize,
}

impl PinnedRemoteDestination {
    /// Pin an existing selected root and existing intermediate directories. The selected
    /// root may itself be a user-selected link; untrusted descendant links are refused.
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
        // Drive allows names Windows cannot create as plain files (`CON`, `a:b`, a trailing
        // dot), which would reach a device or an alternate data stream.
        if segments.iter().any(|segment| {
            !crate::restore_paths::is_normal_component(segment) || !is_windows_file_name(segment)
        }) {
            return Ok(None);
        }
        if create {
            // The root is selected by the user; descendants supplied by Drive are only
            // created and opened one level at a time below it.
            fs::create_dir_all(root)?;
        }
        let mut held = vec![open_directory(root, false)?];
        let mut parent = root.to_path_buf();
        for segment in &segments[..segments.len() - 1] {
            parent.push(segment);
            if create {
                match fs::create_dir(&parent) {
                    Ok(()) => {}
                    // Another process may have created it first; it is checked below.
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error),
                }
            }
            held.push(open_directory(&parent, true)?);
        }
        Ok(Some(Self {
            _held: held,
            parent,
            leaf: segments[segments.len() - 1].to_owned(),
            logical: segments
                .iter()
                .fold(root.to_path_buf(), |path, segment| path.join(segment)),
            depth: segments.len() - 1,
        }))
    }

    pub(crate) fn parent_path(&self) -> PathBuf {
        self.parent.clone()
    }

    pub(crate) fn destination_path(&self) -> PathBuf {
        self.parent.join(&self.leaf)
    }

    pub(crate) fn logical_path(&self) -> &Path {
        &self.logical
    }

    /// A path from which the content of `file`, opened by [`Self::open_existing`], can be read.
    pub(crate) fn source_path(&self, _file: &File) -> PathBuf {
        self.destination_path()
    }

    /// Re-check that no directory below the root has become a link. Renaming or removing
    /// them is already refused by the open handles.
    pub(crate) fn ensure_current(&self) -> io::Result<()> {
        let mut directory = self.parent.as_path();
        for _ in 0..self.depth {
            let metadata = fs::symlink_metadata(directory)?;
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 || !metadata.is_dir()
            {
                return Err(io::Error::other("save directory changed during restore"));
            }
            directory = directory
                .parent()
                .ok_or_else(|| io::Error::other("save has no parent"))?;
        }
        Ok(())
    }

    pub(crate) fn open_existing(&self) -> io::Result<Option<File>> {
        match OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(self.destination_path())
        {
            Ok(file) => {
                let metadata = file.metadata()?;
                if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
                    || !metadata.is_file()
                {
                    return Err(io::Error::other("save is not a regular file"));
                }
                Ok(Some(file))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Exclusively create a previously absent file beneath the pinned parent.
    /// Never replaces an existing save or follows a final link.
    pub fn create_new(&self) -> io::Result<File> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(self.destination_path())
    }
}

/// Open a directory for holding, without granting others the right to delete or rename it.
/// With `refuse_links` the directory itself is opened, not what a link points to.
fn open_directory(path: &Path, refuse_links: bool) -> io::Result<File> {
    let mut flags = FILE_FLAG_BACKUP_SEMANTICS;
    if refuse_links {
        flags |= FILE_FLAG_OPEN_REPARSE_POINT;
    }
    let directory = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(flags)
        .open(path)?;
    let metadata = directory.metadata()?;
    if refuse_links && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other("save directory contains a link"));
    }
    if !metadata.is_dir() {
        return Err(io::Error::other("save directory is not a directory"));
    }
    Ok(directory)
}
