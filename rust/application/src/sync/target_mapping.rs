//! Validation and mapping between local save targets and remote cloud structures.

use emusync_domain::library::{SaveTarget, SaveTargetType};
use emusync_domain::sync::{LocalSaveAccess, LocalSavePath, RemoteFile, remote_path_segments};
use std::collections::{HashMap, HashSet};
use std::fmt::Display;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveTargetMappingError {
    InvalidCloudPath(String),
    ConflictingLocalDestinations,
    FileIsParentOfAnother,
    OverlappingDirectoryScopes,
    ConflictingRemoteToLocalPaths,
    AmbiguousLegacyDirectories(String),
    LocalAccess(String),
}

impl Display for SaveTargetMappingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCloudPath(p) => write!(
                f,
                "Invalid cloud path '{p}'. Use a relative path without '..' or absolute components."
            ),
            Self::ConflictingLocalDestinations => write!(
                f,
                "Different cloud paths cannot restore to the same local save path. Choose separate destinations."
            ),
            Self::FileIsParentOfAnother => write!(
                f,
                "A save file cannot also be a parent directory for another save. Choose separate destinations."
            ),
            Self::OverlappingDirectoryScopes => write!(
                f,
                "Each save directory needs a separate cloud path. Overlapping paths would mix their saves."
            ),
            Self::ConflictingRemoteToLocalPaths => write!(
                f,
                "Cloud files resolve to conflicting local paths. Choose separate destinations before restoring them."
            ),
            Self::AmbiguousLegacyDirectories(msg) => write!(f, "{msg}"),
            Self::LocalAccess(msg) => write!(f, "Local access error: {msg}"),
        }
    }
}

impl std::error::Error for SaveTargetMappingError {}

pub fn project_directory_files(files: &[RemoteFile], prefix: Option<&str>) -> Vec<RemoteFile> {
    let Some(pref) = prefix.filter(|p| !p.trim().is_empty()) else {
        return files.to_vec();
    };
    let prefix_slash = format!("{pref}/");
    files
        .iter()
        .filter(|f| f.name.starts_with(&prefix_slash))
        .map(|f| RemoteFile {
            name: f.name[prefix_slash.len()..].to_string(),
            ..f.clone()
        })
        .collect()
}

pub fn directory_cloud_name(prefix: Option<&str>, relative_path: &str) -> String {
    match prefix.filter(|p| !p.trim().is_empty()) {
        Some(p) => format!("{p}/{relative_path}"),
        None => relative_path.to_string(),
    }
}

pub struct SaveTargetMapping<A> {
    files: A,
}

impl<A> SaveTargetMapping<A>
where
    A: LocalSaveAccess,
    A::Error: std::fmt::Debug,
    <A::Path as LocalSavePath>::Error: std::fmt::Debug,
{
    pub fn new(files: A) -> Self {
        Self { files }
    }

    pub fn files(&self) -> &A {
        &self.files
    }

    pub fn get_sibling_reserved_names(
        &self,
        target_path: &A::Path,
        all_targets: &[(A::Path, SaveTargetType)],
    ) -> Result<HashSet<String>, SaveTargetMappingError> {
        let mut reserved = HashSet::new();
        let target_normalized = target_path
            .normalized_path()
            .map_err(|e| SaveTargetMappingError::LocalAccess(format!("{e:?}")))?;

        for (other_file, other_type) in all_targets {
            let other_normalized = other_file
                .normalized_path()
                .map_err(|e| SaveTargetMappingError::LocalAccess(format!("{e:?}")))?;
            if other_normalized == target_normalized {
                continue;
            }
            let is_other_dir = if other_file
                .exists()
                .map_err(|e| SaveTargetMappingError::LocalAccess(format!("{e:?}")))?
            {
                other_file
                    .is_directory()
                    .map_err(|e| SaveTargetMappingError::LocalAccess(format!("{e:?}")))?
            } else {
                *other_type == SaveTargetType::Directory
            };

            if is_other_dir {
                reserved.extend(self.newer_sibling_paths(other_file, target_path)?);
            } else {
                reserved.insert(other_file.name().to_string());
            }
        }
        Ok(reserved)
    }

    fn newer_sibling_paths(
        &self,
        directory: &A::Path,
        target: &A::Path,
    ) -> Result<HashSet<String>, SaveTargetMappingError> {
        if !directory
            .is_directory()
            .map_err(|e| SaveTargetMappingError::LocalAccess(format!("{e:?}")))?
        {
            return Ok(HashSet::new());
        }
        let walked = directory
            .walk_top_down()
            .map_err(|e| SaveTargetMappingError::LocalAccess(format!("{e:?}")))?;
        let mut result = HashSet::new();
        for file in walked {
            if file
                .is_file()
                .map_err(|e| SaveTargetMappingError::LocalAccess(format!("{e:?}")))?
            {
                let relative = file
                    .relative_to(directory)
                    .map_err(|e| SaveTargetMappingError::LocalAccess(format!("{e:?}")))?;
                let destination = target
                    .resolve(&relative)
                    .map_err(|e| SaveTargetMappingError::LocalAccess(format!("{e:?}")))?;
                let dest_exists = destination
                    .exists()
                    .map_err(|e| SaveTargetMappingError::LocalAccess(format!("{e:?}")))?;
                let newer = if !dest_exists {
                    true
                } else {
                    let file_mod = file
                        .last_modified_ms()
                        .map_err(|e| SaveTargetMappingError::LocalAccess(format!("{e:?}")))?;
                    let dest_mod = destination
                        .last_modified_ms()
                        .map_err(|e| SaveTargetMappingError::LocalAccess(format!("{e:?}")))?;
                    file_mod > dest_mod
                };
                if newer {
                    result.insert(relative);
                }
            }
        }
        Ok(result)
    }

    pub fn is_directory_save_target(
        &self,
        kind: &SaveTargetType,
        path: &A::Path,
        cloud_files: &[RemoteFile],
        reserved_sibling_names: &HashSet<String>,
        cloud_name: Option<&str>,
    ) -> Result<bool, SaveTargetMappingError> {
        if path
            .exists()
            .map_err(|e| SaveTargetMappingError::LocalAccess(format!("{e:?}")))?
        {
            return path
                .is_directory()
                .map_err(|e| SaveTargetMappingError::LocalAccess(format!("{e:?}")));
        }
        match kind {
            SaveTargetType::Directory => Ok(true),
            SaveTargetType::File => Ok(false),
            SaveTargetType::Unknown => {
                let target_name = cloud_name.unwrap_or_else(|| path.name());
                let matches_exact_file = cloud_files
                    .iter()
                    .any(|cf| cf.name == target_name && !reserved_sibling_names.contains(&cf.name));
                Ok(!matches_exact_file)
            }
        }
    }

    pub fn cloud_files_for_save_target<'f>(
        &self,
        kind: &SaveTargetType,
        path: &A::Path,
        cloud_files: &'f [RemoteFile],
        reserved_sibling_names: &HashSet<String>,
    ) -> Result<Vec<&'f RemoteFile>, SaveTargetMappingError> {
        let is_dir =
            self.is_directory_save_target(kind, path, cloud_files, reserved_sibling_names, None)?;
        if !is_dir {
            return Ok(cloud_files.iter().collect());
        }
        Ok(cloud_files
            .iter()
            .filter(|f| !reserved_sibling_names.contains(&f.name))
            .collect())
    }

    pub fn is_directory(
        &self,
        target: &SaveTarget,
        cloud_files: &[RemoteFile],
        siblings: &[SaveTarget],
    ) -> Result<bool, SaveTargetMappingError> {
        let path = self.files.path(&target.path);
        let mut reserved = HashSet::new();
        for s in siblings {
            if s.path != target.path {
                let s_path = self.files.path(&s.path);
                let is_file = s.kind == SaveTargetType::File || s_path.is_file().unwrap_or(false);
                if is_file {
                    reserved.insert(self.file_cloud_name(s));
                }
            }
        }
        self.is_directory_save_target(
            &target.kind,
            &path,
            cloud_files,
            &reserved,
            target.cloud_path.as_deref(),
        )
    }

    pub fn file_cloud_name(&self, target: &SaveTarget) -> String {
        target
            .cloud_path
            .as_deref()
            .filter(|p| !p.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.files.path(&target.path).name().to_string())
    }

    pub fn file_sibling_cloud_names(
        &self,
        target: &SaveTarget,
        siblings: &[SaveTarget],
        cloud_files: &[RemoteFile],
    ) -> Result<HashSet<String>, SaveTargetMappingError> {
        let is_target_dir = self.is_directory(target, cloud_files, siblings)?;
        let prefix = if is_target_dir {
            target
                .cloud_path
                .as_deref()
                .filter(|p| !p.trim().is_empty())
        } else {
            None
        };
        let mut names = HashSet::new();
        for other in siblings {
            if other.path != target.path && !self.is_directory(other, cloud_files, siblings)? {
                let name = self.file_cloud_name(other);
                match prefix {
                    None => {
                        names.insert(name);
                    }
                    Some(p) => {
                        let prefix_slash = format!("{p}/");
                        if let Some(stripped) = name.strip_prefix(&prefix_slash) {
                            names.insert(stripped.to_string());
                        }
                    }
                }
            }
        }
        Ok(names)
    }

    pub fn reserved_cloud_names(
        &self,
        target: &SaveTarget,
        siblings: &[SaveTarget],
        cloud_files: &[RemoteFile],
    ) -> Result<HashSet<String>, SaveTargetMappingError> {
        let is_target_dir = self.is_directory(target, cloud_files, siblings)?;
        let prefix = if is_target_dir {
            target
                .cloud_path
                .as_deref()
                .filter(|p| !p.trim().is_empty())
        } else {
            None
        };
        let mut directories = Vec::new();
        for s in siblings {
            if self.is_directory(s, cloud_files, siblings)? && s.cloud_path == target.cloud_path {
                directories.push((self.files.path(&s.path), s.kind.clone()));
            }
        }
        let target_path = self.files.path(&target.path);
        let mut reserved = self.get_sibling_reserved_names(&target_path, &directories)?;

        for other in siblings {
            if other.path == target.path && other.cloud_path == target.cloud_path {
                continue;
            }
            if !self.is_directory(other, cloud_files, siblings)? {
                let name = self.file_cloud_name(other);
                if let Some(projected) = project_cloud_name(&name, prefix) {
                    reserved.insert(projected);
                }
            } else if let Some(other_cloud) =
                other.cloud_path.as_deref().filter(|p| !p.trim().is_empty())
                && Some(other_cloud) != target.cloud_path.as_deref()
            {
                let other_prefix = format!("{other_cloud}/");
                for file in cloud_files {
                    if file.name.starts_with(&other_prefix)
                        && let Some(projected) = project_cloud_name(&file.name, prefix)
                    {
                        reserved.insert(projected);
                    }
                }
            }
        }
        Ok(reserved)
    }

    pub fn validate_save_target_mappings(
        &self,
        targets: &[SaveTarget],
        cloud_files: &[RemoteFile],
    ) -> Result<(), SaveTargetMappingError> {
        // 1. Validate cloud paths
        for target in targets {
            if let Some(path) = target
                .cloud_path
                .as_deref()
                .filter(|p| !p.trim().is_empty())
                && remote_path_segments(path).is_none()
            {
                return Err(SaveTargetMappingError::InvalidCloudPath(path.to_string()));
            }
        }

        // 2. Validate local destinations
        let mut destinations: HashMap<String, HashSet<String>> = HashMap::new();
        for target in targets {
            let norm = self
                .files
                .path(&target.path)
                .normalized_path()
                .map_err(|e| SaveTargetMappingError::LocalAccess(format!("{e:?}")))?;
            let identity = if self.is_directory(target, cloud_files, targets)? {
                format!("directory:{}", target.cloud_path.as_deref().unwrap_or(""))
            } else {
                format!("file:{}", self.file_cloud_name(target))
            };
            destinations.entry(norm).or_default().insert(identity);
        }
        if destinations.values().any(|set| set.len() > 1) {
            return Err(SaveTargetMappingError::ConflictingLocalDestinations);
        }

        for file_target in targets {
            if !self.is_directory(file_target, cloud_files, targets)? {
                let file_norm = self
                    .files
                    .path(&file_target.path)
                    .normalized_path()
                    .map_err(|e| SaveTargetMappingError::LocalAccess(format!("{e:?}")))?;
                for other in targets {
                    let other_norm = self
                        .files
                        .path(&other.path)
                        .normalized_path()
                        .map_err(|e| SaveTargetMappingError::LocalAccess(format!("{e:?}")))?;
                    if file_norm != other_norm {
                        let ancestors =
                            self.files.path(&other.path).ancestors().map_err(|e| {
                                SaveTargetMappingError::LocalAccess(format!("{e:?}"))
                            })?;
                        if ancestors.contains(&file_norm) {
                            return Err(SaveTargetMappingError::FileIsParentOfAnother);
                        }
                    }
                }
            }
        }

        // 3. Validate directory scopes
        let mut directories = Vec::new();
        for target in targets {
            if self.is_directory(target, cloud_files, targets)? {
                let norm = self
                    .files
                    .path(&target.path)
                    .normalized_path()
                    .map_err(|e| SaveTargetMappingError::LocalAccess(format!("{e:?}")))?;
                if !directories
                    .iter()
                    .any(|(n, cp): &(String, Option<String>)| {
                        *n == norm && *cp == target.cloud_path
                    })
                {
                    directories.push((norm, target.cloud_path.clone()));
                }
            }
        }

        let scoped: Vec<_> = targets
            .iter()
            .filter(|t| {
                self.is_directory(t, cloud_files, targets).unwrap_or(false)
                    && t.cloud_path
                        .as_deref()
                        .is_some_and(|p| !p.trim().is_empty())
            })
            .collect();

        for (i, target) in scoped.iter().enumerate() {
            let target_path = target.cloud_path.as_deref().unwrap();
            let target_slash = format!("{target_path}/");
            for other in scoped.iter().skip(i + 1) {
                let other_path = other.cloud_path.as_deref().unwrap();
                let other_slash = format!("{other_path}/");
                if other_path == target_path
                    || other_path.starts_with(&target_slash)
                    || target_path.starts_with(&other_slash)
                {
                    return Err(SaveTargetMappingError::OverlappingDirectoryScopes);
                }
            }
        }

        // 4. Validate directory files
        for target in targets {
            if self.is_directory(target, cloud_files, targets)? {
                let reserved = self.reserved_cloud_names(target, targets, cloud_files)?;
                let projected = project_directory_files(cloud_files, target.cloud_path.as_deref());
                let target_path = self.files.path(&target.path);

                let mut destination_names: HashMap<String, HashSet<String>> = HashMap::new();
                for file in projected {
                    if !reserved.contains(&file.name)
                        && let Ok(Some(resolved)) = target_path.resolve_remote(&file.name)
                        && let Ok(norm) = resolved.normalized_path()
                    {
                        destination_names
                            .entry(norm)
                            .or_default()
                            .insert(file.name.clone());
                    }
                }
                if destination_names.values().any(|set| set.len() > 1) {
                    return Err(SaveTargetMappingError::ConflictingRemoteToLocalPaths);
                }
                let dest_paths: HashSet<_> = destination_names.keys().cloned().collect();
                for path in &dest_paths {
                    if let Ok(ancestors) = self.files.path(path).ancestors()
                        && ancestors.iter().any(|a| dest_paths.contains(a))
                    {
                        return Err(SaveTargetMappingError::ConflictingRemoteToLocalPaths);
                    }
                }
            }
        }

        // 5. Validate legacy directories
        let legacy: Vec<_> = targets
            .iter()
            .filter(|t| {
                self.is_directory(t, cloud_files, targets).unwrap_or(false)
                    && t.cloud_path.as_deref().is_none_or(|p| p.trim().is_empty())
            })
            .collect();

        if legacy.len() >= 2 {
            let file_names: HashSet<_> = targets
                .iter()
                .filter(|t| !self.is_directory(t, cloud_files, targets).unwrap_or(false))
                .map(|t| self.file_cloud_name(t))
                .collect();
            let legacy_files: Vec<_> = cloud_files
                .iter()
                .filter(|f| {
                    !file_names.contains(&f.name)
                        && !scoped.iter().any(|s| {
                            let prefix = format!("{}/", s.cloud_path.as_deref().unwrap());
                            f.name.starts_with(&prefix)
                        })
                })
                .collect();

            let unavailable_roots: Vec<_> = legacy
                .iter()
                .filter(|t| !self.files.path(&t.path).is_directory().unwrap_or(false))
                .map(|t| t.path.clone())
                .collect();

            let mut local_names_count: HashMap<String, usize> = HashMap::new();
            for target in &legacy {
                let root = self.files.path(&target.path);
                if root.is_directory().unwrap_or(false)
                    && let Ok(walked) = root.walk_top_down()
                {
                    for file in walked {
                        if file.is_file().unwrap_or(false)
                            && let Ok(rel) = file.relative_to(&root)
                        {
                            *local_names_count.entry(rel).or_default() += 1;
                        }
                    }
                }
            }
            let colliding_names: Vec<_> = local_names_count
                .into_iter()
                .filter(|(_, count)| *count > 1)
                .map(|(name, _)| name)
                .collect();

            let unassigned_names: Vec<_> = legacy_files
                .iter()
                .filter(|remote| {
                    !legacy.iter().any(|t| {
                        self.files
                            .path(&t.path)
                            .resolve_remote(&remote.name)
                            .ok()
                            .flatten()
                            .and_then(|p| p.is_file().ok())
                            .unwrap_or(false)
                    })
                })
                .map(|f| f.name.clone())
                .collect();

            let mut problems = Vec::new();
            if !unavailable_roots.is_empty() && !legacy_files.is_empty() {
                problems.push(mapping_details(
                    "Folders unavailable on this device",
                    &unavailable_roots,
                ));
            }
            if !colliding_names.is_empty() {
                problems.push(mapping_details(
                    "Names found in multiple save folders",
                    &colliding_names,
                ));
            }
            if !unassigned_names.is_empty() {
                problems.push(mapping_details(
                    "Cloud files with no matching local save",
                    &unassigned_names,
                ));
            }

            if !problems.is_empty() {
                let configured_paths: Vec<_> = legacy.iter().map(|t| t.path.clone()).collect();
                let configured = mapping_details("Configured save folders", &configured_paths);
                let message = format!(
                    "Cannot safely match cloud saves to these older save folders.\n\n{}\n\n{}\n\nCheck the save paths and folder contents in Edit Game on this device. If restoring to empty folders, restore the cloud files individually to their correct destinations. These folders were not synchronized.",
                    configured,
                    problems.join("\n\n")
                );
                return Err(SaveTargetMappingError::AmbiguousLegacyDirectories(message));
            }
        }

        Ok(())
    }
}

fn project_cloud_name(name: &str, prefix: Option<&str>) -> Option<String> {
    match prefix {
        None => Some(name.to_string()),
        Some(p) => {
            let prefix_slash = format!("{p}/");
            name.strip_prefix(&prefix_slash).map(str::to_string)
        }
    }
}

fn mapping_details(label: &str, paths: &[String]) -> String {
    let shown: Vec<_> = paths.iter().take(5).map(|p| format!("• {p}")).collect();
    let remaining = if paths.len() > 5 {
        format!("\n… ({} more)", paths.len() - 5)
    } else {
        String::new()
    };
    format!("{label}:\n{}{remaining}", shown.join("\n"))
}
