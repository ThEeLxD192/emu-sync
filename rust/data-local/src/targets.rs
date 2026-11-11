use crate::{
    config::{infer, resolve},
    resolve_remote_file,
};
use emusync_domain::library::{Emulator, GameEntry, SaveTarget, SaveTargetAccess, SaveTargetType};
use std::{fs, io, path::Path};

pub struct LocalSaveTargets;
impl SaveTargetAccess for LocalSaveTargets {
    type Error = io::Error;
    fn type_of(&self, path: &str) -> io::Result<Option<SaveTargetType>> {
        if path.trim().is_empty() {
            return Ok(None);
        }
        match fs::metadata(path) {
            Ok(m) => Ok(Some(if m.is_dir() {
                SaveTargetType::Directory
            } else {
                SaveTargetType::File
            })),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
    fn for_entry(&self, entry: &GameEntry) -> io::Result<Vec<SaveTarget>> {
        let c = entry.common();
        let mut targets = if c.save_targets.is_empty() {
            c.save_paths
                .iter()
                .filter(|p| !p.trim().is_empty())
                .map(|p| infer(p))
                .collect()
        } else {
            c.save_targets.clone()
        };
        for target in &mut targets {
            resolve(target);
        }
        Ok(targets)
    }
    fn for_rom(&self, e: &Emulator, rom: &str) -> io::Result<Vec<SaveTarget>> {
        let stem = rom.rsplit_once('.').map_or(rom, |(stem, _)| stem);
        if let Some((_, targets)) = e
            .save_targets_by_rom
            .iter()
            .find(|(k, t)| k == rom && !t.is_empty())
            .or_else(|| {
                e.save_targets_by_rom
                    .iter()
                    .find(|(k, t)| k == stem && !t.is_empty())
            })
        {
            let mut targets = targets.clone();
            targets.iter_mut().for_each(resolve);
            return Ok(targets);
        }
        let has_non_empty_paths = |paths: &[String]| paths.iter().any(|p| !p.trim().is_empty());
        Ok(e.save_paths_by_rom
            .iter()
            .find(|(k, paths)| k == rom && has_non_empty_paths(paths))
            .or_else(|| {
                e.save_paths_by_rom
                    .iter()
                    .find(|(k, paths)| k == stem && has_non_empty_paths(paths))
            })
            .map(|(_, paths)| {
                paths
                    .iter()
                    .filter(|p| !p.trim().is_empty())
                    .map(|p| infer(p))
                    .collect()
            })
            .unwrap_or_default())
    }
    fn for_restore(&self, destination: &str, names: &[String]) -> io::Result<Vec<SaveTarget>> {
        if destination.trim().is_empty() {
            return Ok(Vec::new());
        }
        let mut seen = std::collections::HashSet::<&str>::with_capacity(names.len());
        let mut targets = Vec::with_capacity(names.len());
        for name in names {
            if !seen.insert(name.as_str()) {
                continue;
            }
            if let Some(path) = resolve_remote_file(Path::new(destination), name)? {
                let path = path
                    .to_str()
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidInput, "path is not UTF-8")
                    })?
                    .to_owned();
                targets.push(SaveTarget {
                    path,
                    kind: SaveTargetType::File,
                    cloud_path: Some(name.clone()),
                    excluded_folders: Vec::new(),
                });
            }
        }
        Ok(targets)
    }
}
