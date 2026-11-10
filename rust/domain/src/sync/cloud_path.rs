use std::collections::BTreeSet;

pub fn sanitize_cloud_folder_name(name: &str) -> String {
    let name = name.replace(['/', '\\'], "-");
    let name = name.trim();
    if name.is_empty() {
        "Unknown".into()
    } else {
        name.into()
    }
}
fn normalize_save_path(path: &str) -> String {
    let replaced = path.trim().replace('\\', "/");
    let trimmed = replaced.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".into()
    } else {
        trimmed.into()
    }
}
pub fn cloud_path_segments(
    entry: &crate::library::GameEntry,
    rom_name: Option<&str>,
    game_title: Option<&str>,
    global: &[crate::library::SaveTarget],
    per_rom: &[crate::library::SaveTarget],
) -> Vec<String> {
    use crate::library::GameEntry;
    let common = entry.common();
    let mut segments = vec![common.effective_cloud_folder().to_owned()];
    match entry {
        GameEntry::Emulator(emulator) => {
            let global_paths: BTreeSet<_> = global
                .iter()
                .map(|t| (normalize_save_path(&t.path), &t.cloud_path))
                .collect();
            let distinct = per_rom
                .iter()
                .any(|t| !global_paths.contains(&(normalize_save_path(&t.path), &t.cloud_path)));
            if let Some(rom) = rom_name.filter(|_| distinct || global_paths.is_empty()) {
                let title = game_title
                    .map(str::to_owned)
                    .unwrap_or_else(|| emulator.effective_title(rom, rom));
                segments.push(sanitize_cloud_folder_name(&title));
            }
        }
        GameEntry::Native(_) if common.group.is_some() && common.cloud_folder.is_none() => {
            segments.push(sanitize_cloud_folder_name(&common.name))
        }
        GameEntry::Native(_) => {}
    }
    segments
}
