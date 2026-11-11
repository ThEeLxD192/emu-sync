//! Save target form rows and validation.

use emusync_domain::{
    library::{SaveTarget, SaveTargetType, scope_new_targets},
    sync::remote_path_segments,
};
use std::path::{Component, Path};

/// An optional cloud path is valid when empty or relative without `..` segments.
pub fn cloud_path_is_valid(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.is_empty() || remote_path_segments(trimmed).is_some()
}

/// An excluded folder is valid when empty (ignored) or relative without `..` segments, so
/// it always names a folder inside its save folder.
pub fn excluded_folder_is_valid(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.is_empty() || remote_path_segments(trimmed).is_some()
}

/// The folder `picked` relative to the save folder `root`, with `/` separators, when it
/// lies strictly inside it; the save folder itself cannot be excluded.
pub fn excluded_folder_within(root: &str, picked: &str) -> Option<String> {
    let root = root.trim();
    if root.is_empty() {
        return None;
    }
    let relative = Path::new(picked.trim()).strip_prefix(root).ok()?;
    let segments: Vec<&str> = relative
        .components()
        .map(|c| match c {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect::<Option<_>>()?;
    (!segments.is_empty()).then(|| segments.join("/"))
}

pub(crate) const EXCLUDED_FOLDERS_INVALID: &str =
    "Excluded folders must be inside their save folder.";

/// An editable row representing a save target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveTargetRow {
    pub path: String,
    pub target_type: SaveTargetType,
    pub cloud_path: String,
    /// Folders inside a directory save, relative to it, that are never synced.
    pub excluded_folders: Vec<String>,
    /// Loaded from the configuration: its cloud layout is in use and is never changed
    /// automatically, even if its path is edited.
    pub established: bool,
}

impl SaveTargetRow {
    pub fn new(path: String, target_type: SaveTargetType, cloud_path: String) -> Self {
        Self {
            path,
            target_type,
            cloud_path,
            excluded_folders: Vec::new(),
            established: false,
        }
    }

    /// Only a folder can hold excluded folders; a file save ignores them.
    pub fn accepts_excluded_folders(&self) -> bool {
        self.target_type != SaveTargetType::File
    }

    fn target(&self) -> Option<SaveTarget> {
        let path = self.path.trim();
        if path.is_empty() {
            return None;
        }
        let cloud = self.cloud_path.trim();
        Some(SaveTarget {
            path: path.to_string(),
            kind: self.target_type.clone(),
            cloud_path: (!cloud.is_empty()).then(|| cloud.to_string()),
            excluded_folders: if self.accepts_excluded_folders() {
                self.excluded_folders
                    .iter()
                    .map(|f| f.trim())
                    .filter(|f| !f.is_empty())
                    .map(str::to_string)
                    .collect()
            } else {
                Vec::new()
            },
        })
    }
}

/// State for editing a list of save targets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveTargetFormState {
    pub rows: Vec<SaveTargetRow>,
}

impl Default for SaveTargetFormState {
    fn default() -> Self {
        Self {
            rows: vec![SaveTargetRow::new(
                String::new(),
                SaveTargetType::File,
                String::new(),
            )],
        }
    }
}

impl SaveTargetFormState {
    pub fn from_targets(targets: &[SaveTarget]) -> Self {
        if targets.is_empty() {
            return Self::default();
        }
        let rows = targets
            .iter()
            .map(|t| SaveTargetRow {
                excluded_folders: t.excluded_folders.clone(),
                established: true,
                ..SaveTargetRow::new(
                    t.path.clone(),
                    t.kind.clone(),
                    t.cloud_path.clone().unwrap_or_default(),
                )
            })
            .collect();
        Self { rows }
    }

    pub fn add_row(&mut self) {
        self.rows.push(SaveTargetRow::new(
            String::new(),
            SaveTargetType::File,
            String::new(),
        ));
    }

    pub fn remove_row(&mut self, index: usize) {
        if index < self.rows.len() {
            self.rows.remove(index);
        }
        if self.rows.is_empty() {
            self.add_row();
        }
    }

    pub fn toggle_type(&mut self, index: usize) {
        if let Some(row) = self.rows.get_mut(index) {
            row.target_type = match row.target_type {
                SaveTargetType::File => SaveTargetType::Directory,
                SaveTargetType::Directory | SaveTargetType::Unknown => SaveTargetType::File,
            };
        }
    }

    pub fn update_path(&mut self, index: usize, path: String) {
        if let Some(row) = self.rows.get_mut(index) {
            row.path = path;
        }
    }

    pub fn update_type(&mut self, index: usize, target_type: SaveTargetType) {
        if let Some(row) = self.rows.get_mut(index) {
            row.target_type = target_type;
        }
    }

    pub fn update_cloud_path(&mut self, index: usize, cloud_path: String) {
        if let Some(row) = self.rows.get_mut(index) {
            row.cloud_path = cloud_path;
        }
    }

    pub fn cloud_paths_are_valid(&self) -> bool {
        self.rows.iter().all(|r| cloud_path_is_valid(&r.cloud_path))
    }

    pub fn add_excluded_folder(&mut self, index: usize) {
        if let Some(row) = self.rows.get_mut(index) {
            row.excluded_folders.push(String::new());
        }
    }

    pub fn remove_excluded_folder(&mut self, index: usize, folder: usize) {
        if let Some(row) = self.rows.get_mut(index)
            && folder < row.excluded_folders.len()
        {
            row.excluded_folders.remove(folder);
        }
    }

    pub fn update_excluded_folder(&mut self, index: usize, folder: usize, value: String) {
        if let Some(slot) = self
            .rows
            .get_mut(index)
            .and_then(|row| row.excluded_folders.get_mut(folder))
        {
            *slot = value;
        }
    }

    /// Stores a folder chosen in the picker relative to the row's save folder. One
    /// outside it is kept as picked, so the field shows it as invalid instead of
    /// silently dropping the choice.
    pub fn pick_excluded_folder(&mut self, index: usize, folder: usize, picked: String) {
        let Some(row) = self.rows.get(index) else {
            return;
        };
        let value = excluded_folder_within(&row.path, &picked).unwrap_or(picked);
        self.update_excluded_folder(index, folder, value);
    }

    /// Every excluded folder of a row that is saved and accepts them names a folder
    /// inside it; rows without a path are dropped on saving, so they never block it.
    pub fn excluded_folders_are_valid(&self) -> bool {
        self.rows
            .iter()
            .filter(|r| !r.path.trim().is_empty() && r.accepts_excluded_folders())
            .all(|r| {
                r.excluded_folders
                    .iter()
                    .all(|f| excluded_folder_is_valid(f))
            })
    }

    /// The first problem that keeps these targets from being saved, if any.
    pub fn validation_error(&self) -> Option<&'static str> {
        if !self.cloud_paths_are_valid() {
            Some("Cloud paths must be relative and cannot contain '..'.")
        } else if !self.excluded_folders_are_valid() {
            Some(EXCLUDED_FOLDERS_INVALID)
        } else {
            None
        }
    }

    pub fn build_targets(&self) -> Vec<SaveTarget> {
        self.rows.iter().filter_map(SaveTargetRow::target).collect()
    }

    /// The targets to save: new directories get their own cloud prefix when the list
    /// holds two or more directories, so their files never mix in the cloud.
    pub fn scoped_targets(&self) -> Vec<SaveTarget> {
        let (targets, is_new): (Vec<_>, Vec<_>) = self
            .rows
            .iter()
            .filter_map(|row| row.target().map(|target| (target, !row.established)))
            .unzip();
        scope_new_targets(&targets, &is_new)
    }

    /// Per row, the cloud path saving will assign because the row left it empty.
    pub fn automatic_cloud_paths(&self) -> Vec<Option<String>> {
        let mut scoped = self.scoped_targets().into_iter();
        self.rows
            .iter()
            .map(|row| {
                let target = row.target()?;
                let saved = scoped.next()?;
                saved.cloud_path.filter(|_| target.cloud_path.is_none())
            })
            .collect()
    }
}
