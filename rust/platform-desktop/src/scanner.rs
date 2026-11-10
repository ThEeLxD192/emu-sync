use std::{
    collections::{HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
};

const GAME_SIGNATURES: &[&str] = &["ps3_disc", "eboot", "app", "main"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RomFile {
    pub file: PathBuf,
    pub name: String,
    pub extension: String,
}

struct DirectoryScan {
    roms: Vec<RomFile>,
    subdirectories: Vec<PathBuf>,
}

/// Scans a directory for ROM files matching the given extensions.
/// Avoids symlink loops by resolving real canonical paths.
pub fn scan_roms(directory: &Path, extensions: &[String]) -> Vec<RomFile> {
    if !directory.is_dir() {
        return Vec::new();
    }

    let allowed_extensions: HashSet<String> = extensions
        .iter()
        .map(|ext| ext.trim().trim_start_matches('.').to_lowercase())
        .filter(|ext| !ext.is_empty())
        .collect();

    if allowed_extensions.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    let mut visited = HashSet::new();
    let mut pending = VecDeque::new();
    pending.push_back(directory.to_path_buf());

    while let Some(dir) = pending.pop_front() {
        let real_path = match fs::canonicalize(&dir) {
            Ok(p) => p,
            Err(_) => continue,
        };

        if !visited.insert(real_path) {
            continue;
        }

        let children = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        let scan = scan_directory(&dir, children, &allowed_extensions);
        result.extend(scan.roms);
        pending.extend(scan.subdirectories);
    }

    // Deduplicate by absolute path, sort alphabetically by name (lowercase)
    let mut seen_paths = HashSet::new();
    let mut unique_roms = Vec::new();
    for rom in result {
        if seen_paths.insert(rom.file.clone()) {
            unique_roms.push(rom);
        }
    }

    unique_roms.sort_by_cached_key(|a| a.name.to_lowercase());
    unique_roms
}

fn scan_directory(
    directory: &Path,
    entries: fs::ReadDir,
    allowed_extensions: &HashSet<String>,
) -> DirectoryScan {
    let mut roms = Vec::new();
    let mut subdirectories = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            subdirectories.push(path);
        } else if path.is_file() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            if allowed_extensions.contains(&ext) {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();

                if GAME_SIGNATURES.contains(&stem.to_lowercase().as_str()) {
                    let dir_name = directory
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();
                    return DirectoryScan {
                        roms: vec![RomFile {
                            file: directory.to_path_buf(),
                            name: dir_name,
                            extension: ext,
                        }],
                        subdirectories: Vec::new(),
                    };
                }

                roms.push(RomFile {
                    file: path,
                    name: stem,
                    extension: ext,
                });
            }
        }
    }

    DirectoryScan {
        roms,
        subdirectories,
    }
}
