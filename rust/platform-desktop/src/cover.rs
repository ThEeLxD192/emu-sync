use emusync_domain::library::{CoverFiles, GameEntry, clean_game_title};
use std::path::{Path, PathBuf};

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp"];

#[derive(Clone, Copy, Debug, Default)]
pub struct DesktopCoverFiles;

impl CoverFiles for DesktopCoverFiles {
    type Error = std::io::Error;

    fn find(
        &self,
        name: &str,
        entry: &GameEntry,
        rom_path: Option<&str>,
    ) -> Result<Option<String>, Self::Error> {
        Ok(find_cover(name, entry, rom_path).map(|p| p.to_string_lossy().to_string()))
    }
}

fn find_cover(name: &str, entry: &GameEntry, rom_path: Option<&str>) -> Option<PathBuf> {
    let rom_path_buf = rom_path.map(PathBuf::from);
    let rom_file = rom_path_buf.as_deref();

    // 1. Check custom path
    if let Some(custom) = find_custom_cover(entry, rom_file) {
        return Some(custom);
    }

    let base_name = rom_file
        .and_then(|p| p.file_stem())
        .and_then(|s| s.to_str())
        .unwrap_or(name)
        .to_string();

    let clean_name = clean_game_title(&base_name);
    let space_name = base_name.replace('_', " ").trim().to_string();

    let rom_directory = rom_file
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
        .or_else(|| match entry {
            GameEntry::Native(native) => Path::new(&native.common.executable_path)
                .parent()
                .map(Path::to_path_buf),
            _ => None,
        });

    // 2. Check adjacent cover in ROM directory
    if let Some(ref dir) = rom_directory
        && dir.exists()
        && let Some(cover) = find_adjacent_cover(
            dir,
            entry,
            rom_file,
            name,
            &base_name,
            &clean_name,
            &space_name,
        )
    {
        return Some(cover);
    }

    // 3. Fallbacks by entry type
    match entry {
        GameEntry::Emulator(emu) => find_emulator_cover(
            emu,
            rom_directory.as_deref(),
            rom_file,
            &base_name,
            &clean_name,
            &space_name,
        ),
        GameEntry::Native(native) => {
            let dir = Path::new(&native.common.executable_path).parent()?;
            find_native_cover(dir, name, &clean_name)
        }
    }
}

fn find_custom_cover(entry: &GameEntry, rom_file: Option<&Path>) -> Option<PathBuf> {
    match entry {
        GameEntry::Emulator(emu) => {
            let rom_name = rom_file
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())?;
            emu.cover_path_by_rom
                .iter()
                .find(|(key, _)| key == rom_name)
                .map(|(_, path)| PathBuf::from(path.trim()))
                .filter(|p| !p.as_os_str().is_empty() && p.is_file())
        }
        GameEntry::Native(native) => native
            .cover_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_file()),
    }
}

fn find_adjacent_cover(
    dir: &Path,
    entry: &GameEntry,
    rom_file: Option<&Path>,
    name: &str,
    base_name: &str,
    clean_name: &str,
    space_name: &str,
) -> Option<PathBuf> {
    let title = match entry {
        GameEntry::Emulator(emu) => {
            let rom_name = rom_file
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or(name);
            emu.effective_title(rom_name, name)
        }
        GameEntry::Native(native) => native.common.name.clone(),
    };

    let rom_name_str = rom_file
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str());

    let mut names = vec![base_name, title.as_str(), clean_name, space_name];
    if let Some(rn) = rom_name_str {
        names.push(rn);
    }
    // Deduplica manteniendo el orden de primera aparición.
    let mut seen = std::collections::HashSet::new();
    names.retain(|n| seen.insert(*n));

    for candidate in &names {
        for ext in IMAGE_EXTENSIONS {
            let candidate_path = dir.join(format!("{candidate}.{ext}"));
            if candidate_path.is_file() {
                return Some(candidate_path);
            }
            let upper_ext = ext.to_uppercase();
            let upper_path = dir.join(format!("{candidate}.{upper_ext}"));
            if upper_path.is_file() {
                return Some(upper_path);
            }
        }
    }

    // Case-insensitive fallback
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if is_image_file(&path)
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                && stem.eq_ignore_ascii_case(base_name)
            {
                return Some(path);
            }
        }
    }

    None
}

fn find_emulator_cover(
    emu: &emusync_domain::library::Emulator,
    rom_directory: Option<&Path>,
    rom_file: Option<&Path>,
    base_name: &str,
    clean_name: &str,
    space_name: &str,
) -> Option<PathBuf> {
    let mut directories = Vec::new();

    if let Some(ref custom_dir) = emu.covers_directory
        && !custom_dir.trim().is_empty()
    {
        let path = PathBuf::from(custom_dir.trim());
        if path.exists() {
            directories.push(path);
        }
    }

    if let Some(rom_dir) = rom_directory
        && rom_dir.exists()
    {
        directories.push(rom_dir.join("covers"));
        directories.push(rom_dir.join("boxart"));
        directories.push(rom_dir.join("images"));
    }

    let mut names = vec![base_name];
    if !clean_name.is_empty() && clean_name != base_name {
        names.push(clean_name);
    }
    if !space_name.is_empty() && space_name != base_name && space_name != clean_name {
        names.push(space_name);
    }
    if let Some(rn) = rom_file
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
    {
        names.push(rn);
    }

    for dir in directories {
        if !dir.exists() {
            continue;
        }

        for candidate in &names {
            for ext in IMAGE_EXTENSIONS {
                let candidate_path = dir.join(format!("{candidate}.{ext}"));
                if candidate_path.exists() {
                    return Some(candidate_path);
                }
                let upper_ext = ext.to_uppercase();
                let upper_path = dir.join(format!("{candidate}.{upper_ext}"));
                if upper_path.exists() {
                    return Some(upper_path);
                }
            }
        }

        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if is_image_file(&path)
                    && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                    && names.iter().any(|n| stem.eq_ignore_ascii_case(n))
                {
                    return Some(path);
                }
            }
        }
    }

    None
}

fn find_native_cover(dir: &Path, name: &str, clean_name: &str) -> Option<PathBuf> {
    if !dir.exists() {
        return None;
    }

    let candidates = [name, clean_name, "cover", "boxart"];
    for candidate in &candidates {
        for ext in IMAGE_EXTENSIONS {
            let candidate_path = dir.join(format!("{candidate}.{ext}"));
            if candidate_path.exists() {
                return Some(candidate_path);
            }
            let upper_ext = ext.to_uppercase();
            let upper_path = dir.join(format!("{candidate}.{upper_ext}"));
            if upper_path.exists() {
                return Some(upper_path);
            }
        }
    }

    None
}

fn is_image_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| {
            IMAGE_EXTENSIONS
                .iter()
                .any(|&e| ext.eq_ignore_ascii_case(e))
        })
}
