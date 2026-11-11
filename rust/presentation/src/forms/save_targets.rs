//! Save target form rows and validation.

use emusync_domain::{
    library::{SaveTarget, SaveTargetType, scope_new_targets},
    sync::remote_path_segments,
};

/// An optional cloud path is valid when empty or relative without `..` segments.
pub fn cloud_path_is_valid(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.is_empty() || remote_path_segments(trimmed).is_some()
}

/// An editable row representing a save target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveTargetRow {
    pub path: String,
    pub target_type: SaveTargetType,
    pub cloud_path: String,
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
            established: false,
        }
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
