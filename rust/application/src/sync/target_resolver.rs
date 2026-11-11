//! Resolves game and ROM configurations to concrete local and cloud targets.

use emusync_domain::library::{GameEntry, SaveTarget, SaveTargetType};
use emusync_domain::sync::{
    LocalSaveAccess, LocalSavePath, SyncGame, cloud_path_segments, sanitize_cloud_folder_name,
};
use std::collections::{BTreeSet, HashSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedSaveTarget<P> {
    pub file: P,
    pub kind: SaveTargetType,
    pub cloud_path_segments: Vec<String>,
    pub display_name: String,
    pub cloud_path: Option<String>,
}

impl<P: LocalSavePath> ResolvedSaveTarget<P> {
    pub fn configured(&self) -> SaveTarget {
        SaveTarget {
            path: self.file.path().to_string(),
            kind: self.kind.clone(),
            cloud_path: self.cloud_path.clone(),
        }
    }
}

pub struct SaveTargetResolver<A> {
    files: A,
}

impl<A> SaveTargetResolver<A> {
    pub const fn new(files: A) -> Self {
        Self { files }
    }
}

impl<A: LocalSaveAccess> SaveTargetResolver<A> {
    pub fn get_cloud_path_segments_entry(
        &self,
        entry: &GameEntry,
        rom_name: Option<&str>,
        game_title: Option<&str>,
    ) -> Result<Vec<String>, A::Error> {
        let global = self.files.effective_targets(entry)?;
        let per_rom = match entry {
            GameEntry::Emulator(emu) => {
                if let Some(rom) = rom_name {
                    self.files.effective_targets_for_rom(emu, rom)?
                } else {
                    Vec::new()
                }
            }
            GameEntry::Native(_) => Vec::new(),
        };
        Ok(cloud_path_segments(
            entry, rom_name, game_title, &global, &per_rom,
        ))
    }

    pub fn get_cloud_path_segments_game(&self, item: &SyncGame) -> Result<Vec<String>, A::Error> {
        let path_obj;
        let rom_name = match &item.rom_path {
            Some(p) => {
                path_obj = self.files.path(p);
                path_obj.name()
            }
            None => &item.name,
        };
        self.get_cloud_path_segments_entry(&item.entry, Some(rom_name), Some(&item.effective_title))
    }

    pub fn get_save_targets(
        &self,
        entry: &GameEntry,
    ) -> Result<Vec<ResolvedSaveTarget<A::Path>>, A::Error> {
        match entry {
            GameEntry::Native(native) => {
                let effective = self.files.effective_targets(entry)?;
                if effective.is_empty() {
                    return Ok(Vec::new());
                }
                let cloud_path = cloud_path_segments(entry, None, None, &effective, &[]);
                Ok(effective
                    .into_iter()
                    .map(|t| ResolvedSaveTarget {
                        file: self.files.path(&t.path),
                        kind: t.kind,
                        cloud_path_segments: cloud_path.clone(),
                        display_name: native.common.name.clone(),
                        cloud_path: t.cloud_path,
                    })
                    .collect())
            }
            GameEntry::Emulator(emu) => {
                let mut targets = Vec::new();
                let mut seen = HashSet::new();
                let base_folder = emu.common.effective_cloud_folder();
                let effective = self.files.effective_targets(entry)?;
                if !effective.is_empty() {
                    let global_path = vec![base_folder.to_string()];
                    for target in effective {
                        let file = self.files.path(&target.path);
                        let abs = file
                            .absolute_path()
                            .unwrap_or_else(|_| file.path().to_string());
                        if seen.insert((abs, target.cloud_path.clone())) {
                            targets.push(ResolvedSaveTarget {
                                file,
                                kind: target.kind,
                                cloud_path_segments: global_path.clone(),
                                display_name: emu.common.name.clone(),
                                cloud_path: target.cloud_path,
                            });
                        }
                    }
                }
                let mut rom_names = BTreeSet::new();
                for k in emu
                    .save_targets_by_rom
                    .iter()
                    .map(|(k, _)| k.as_str())
                    .chain(emu.save_paths_by_rom.iter().map(|(k, _)| k.as_str()))
                {
                    rom_names.insert(k);
                }
                for rom_name in rom_names {
                    let rom_targets = self.files.effective_targets_for_rom(emu, rom_name)?;
                    if rom_targets.is_empty() {
                        continue;
                    }
                    let title = emu.effective_title(rom_name, rom_name);
                    let rom_cloud_path =
                        vec![base_folder.to_string(), sanitize_cloud_folder_name(&title)];
                    for target in rom_targets {
                        let file = self.files.path(&target.path);
                        let abs = file
                            .absolute_path()
                            .unwrap_or_else(|_| file.path().to_string());
                        if seen.insert((abs, target.cloud_path.clone())) {
                            targets.push(ResolvedSaveTarget {
                                file,
                                kind: target.kind,
                                cloud_path_segments: rom_cloud_path.clone(),
                                display_name: title.clone(),
                                cloud_path: target.cloud_path,
                            });
                        }
                    }
                }
                Ok(targets)
            }
        }
    }
}
