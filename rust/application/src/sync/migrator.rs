//! Cloud folder migration with duplicate checks and rollback on failure or cancellation.

use emusync_domain::sync::{CloudFileListing, CloudFolderStore, RemoteFile};
use std::collections::{BTreeSet, HashSet};
use std::fmt::Display;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MigrationError {
    Folder(String),
    Listing(String),
    DuplicateNames(String),
    MoveFailed {
        reason: String,
        rollback_failures: Vec<String>,
        old_path: Vec<String>,
        new_path: Vec<String>,
    },
    Cancelled {
        rollback_failures: Vec<String>,
        old_path: Vec<String>,
        new_path: Vec<String>,
    },
}

fn format_recovery(
    rollback_failures: &[String],
    old_path: &[String],
    new_path: &[String],
) -> String {
    if rollback_failures.is_empty() {
        format!(
            "Previously moved files were restored to '{}'.",
            old_path.join("/")
        )
    } else {
        format!(
            "Some files remain in '{}': {}.",
            new_path.join("/"),
            rollback_failures.join(", ")
        )
    }
}

impl Display for MigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Folder(e) => write!(f, "Cloud folder error: {e}"),
            Self::Listing(e) => write!(f, "Cloud listing error: {e}"),
            Self::DuplicateNames(msg) => write!(f, "{msg}"),
            Self::MoveFailed {
                reason,
                rollback_failures,
                old_path,
                new_path,
            } => {
                let recovery = format_recovery(rollback_failures, old_path, new_path);
                write!(
                    f,
                    "Cloud migration failed: {reason}. {recovery} No folders were deleted."
                )
            }
            Self::Cancelled {
                rollback_failures,
                old_path,
                new_path,
            } => {
                let recovery = format_recovery(rollback_failures, old_path, new_path);
                write!(
                    f,
                    "Cloud migration cancelled. {recovery} No folders were deleted."
                )
            }
        }
    }
}

impl std::error::Error for MigrationError {}

pub struct CloudFolderMigrator<S, L> {
    folders: S,
    search: L,
}

impl<S, L> CloudFolderMigrator<S, L>
where
    S: CloudFolderStore,
    S::Error: std::fmt::Debug,
    L: CloudFileListing,
    L::Error: std::fmt::Debug,
{
    pub fn new(folders: S, search: L) -> Self {
        Self { folders, search }
    }

    pub async fn migrate(
        &self,
        token: &str,
        old_path: &[String],
        new_path: &[String],
    ) -> Result<(), MigrationError> {
        self.migrate_with_cancel(token, old_path, new_path, || false)
            .await
    }

    pub async fn migrate_with_cancel(
        &self,
        token: &str,
        old_path: &[String],
        new_path: &[String],
        is_cancelled: impl Fn() -> bool,
    ) -> Result<(), MigrationError> {
        if old_path == new_path {
            return Ok(());
        }
        let source = self
            .folders
            .find_path(token, old_path)
            .await
            .map_err(|e| MigrationError::Folder(format!("{e:?}")))?;
        let Some(source_id) = source else {
            return Ok(());
        };

        let destination = self
            .folders
            .find_path(token, new_path)
            .await
            .map_err(|e| MigrationError::Folder(format!("{e:?}")))?;

        if destination.as_deref() == Some(&source_id) {
            return Ok(());
        }

        match destination {
            None => self.relocate(token, &source_id, old_path, new_path).await,
            Some(dest_id) => {
                self.merge(
                    token,
                    &source_id,
                    &dest_id,
                    old_path,
                    new_path,
                    is_cancelled,
                )
                .await
            }
        }
    }

    async fn relocate(
        &self,
        token: &str,
        source: &str,
        old_path: &[String],
        new_path: &[String],
    ) -> Result<(), MigrationError> {
        let old_parent = &old_path[..old_path.len().saturating_sub(1)];
        let new_parent = &new_path[..new_path.len().saturating_sub(1)];
        let folder_name = new_path.last().map(String::as_str).unwrap_or("");

        if old_parent == new_parent {
            self.folders
                .rename_folder(token, source, folder_name, None, None)
                .await
                .map_err(|e| MigrationError::Folder(format!("{e:?}")))?;
        } else {
            let add_parent = self
                .folders
                .ensure_path(token, new_parent)
                .await
                .map_err(|e| MigrationError::Folder(format!("{e:?}")))?;
            let remove_parent = self
                .folders
                .find_path(token, old_parent)
                .await
                .map_err(|e| MigrationError::Folder(format!("{e:?}")))?;
            self.folders
                .rename_folder(
                    token,
                    source,
                    folder_name,
                    Some(&add_parent),
                    remove_parent.as_deref(),
                )
                .await
                .map_err(|e| MigrationError::Folder(format!("{e:?}")))?;
        }
        Ok(())
    }

    async fn merge(
        &self,
        token: &str,
        source: &str,
        destination: &str,
        old_path: &[String],
        new_path: &[String],
        is_cancelled: impl Fn() -> bool,
    ) -> Result<(), MigrationError> {
        let source_files = self
            .search
            .list_children(token, source)
            .await
            .map_err(|e| MigrationError::Listing(format!("{e:?}")))?;
        let destination_files = self
            .search
            .list_children(token, destination)
            .await
            .map_err(|e| MigrationError::Listing(format!("{e:?}")))?;

        let mut source_names = HashSet::with_capacity(source_files.len());
        let mut collisions = BTreeSet::new();

        for file in &source_files {
            if !source_names.insert(&file.name) {
                collisions.insert(file.name.clone());
            }
        }

        let dest_names: HashSet<&str> = destination_files.iter().map(|f| f.name.as_str()).collect();
        for file in &source_files {
            if dest_names.contains(file.name.as_str()) {
                collisions.insert(file.name.clone());
            }
        }

        if !collisions.is_empty() {
            let collision_list: Vec<String> = collisions.into_iter().collect();
            let msg = format!(
                "Cloud folders '{}' and '{}' contain duplicate names: {}. Both copies were kept. Resolve the duplicates in Google Drive before renaming.",
                old_path.join("/"),
                new_path.join("/"),
                collision_list.join(", ")
            );
            return Err(MigrationError::DuplicateNames(msg));
        }

        self.move_with_rollback(
            token,
            source,
            destination,
            &source_files,
            old_path,
            new_path,
            is_cancelled,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn move_with_rollback(
        &self,
        token: &str,
        source: &str,
        destination: &str,
        source_files: &[RemoteFile],
        old_path: &[String],
        new_path: &[String],
        is_cancelled: impl Fn() -> bool,
    ) -> Result<(), MigrationError> {
        let mut moved: Vec<&RemoteFile> = Vec::new();
        let mut failure_reason = None;
        let mut was_cancelled = false;

        for file in source_files {
            if is_cancelled() {
                was_cancelled = true;
                break;
            }
            match self
                .folders
                .move_file(token, &file.id, destination, source)
                .await
            {
                Ok(()) => {
                    moved.push(file);
                }
                Err(err) => {
                    failure_reason = Some(format!("{err:?}"));
                    break;
                }
            }
        }

        if was_cancelled || failure_reason.is_some() {
            let rollback_failures = self
                .rollback_moves(token, source, destination, &moved)
                .await;
            if was_cancelled {
                return Err(MigrationError::Cancelled {
                    rollback_failures,
                    old_path: old_path.to_vec(),
                    new_path: new_path.to_vec(),
                });
            }
            return Err(MigrationError::MoveFailed {
                reason: failure_reason.unwrap_or_else(|| "Unknown move error".into()),
                rollback_failures,
                old_path: old_path.to_vec(),
                new_path: new_path.to_vec(),
            });
        }

        Ok(())
    }

    async fn rollback_moves(
        &self,
        token: &str,
        source: &str,
        destination: &str,
        moved: &[&RemoteFile],
    ) -> Vec<String> {
        let mut failures = Vec::new();
        for file in moved.iter().rev() {
            if self
                .folders
                .move_file(token, &file.id, source, destination)
                .await
                .is_err()
            {
                failures.push(file.name.clone());
            }
        }
        failures
    }
}
