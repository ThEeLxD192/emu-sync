//! Save target form rows and validation.

use emusync_domain::{
    library::{SaveTarget, SaveTargetType},
    sync::remote_path_segments,
};

/// An editable row representing a save target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveTargetRow {
    pub path: String,
    pub target_type: SaveTargetType,
    pub cloud_path: String,
}

impl SaveTargetRow {
    pub fn new(path: String, target_type: SaveTargetType, cloud_path: String) -> Self {
        Self {
            path,
            target_type,
            cloud_path,
        }
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
            .map(|t| {
                SaveTargetRow::new(
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
        self.rows.iter().all(|r| {
            let trimmed = r.cloud_path.trim();
            trimmed.is_empty() || remote_path_segments(trimmed).is_some()
        })
    }

    pub fn build_targets(&self) -> Vec<SaveTarget> {
        self.rows
            .iter()
            .filter_map(|r| {
                let trimmed_path = r.path.trim();
                if trimmed_path.is_empty() {
                    None
                } else {
                    let trimmed_cloud = r.cloud_path.trim();
                    let cloud_path = if trimmed_cloud.is_empty() {
                        None
                    } else {
                        Some(trimmed_cloud.to_string())
                    };
                    Some(SaveTarget {
                        path: trimmed_path.to_string(),
                        kind: r.target_type.clone(),
                        cloud_path,
                    })
                }
            })
            .collect()
    }
}
