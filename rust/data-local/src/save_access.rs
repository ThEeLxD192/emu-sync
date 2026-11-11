use crate::{LocalSaveTargets, resolve_remote_file, storage::absolute};
use emusync_domain::{
    library::{Emulator, GameEntry, SaveTarget, SaveTargetAccess},
    sync::{LocalSaveAccess, LocalSavePath},
};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct DesktopSaveAccess;
#[derive(Clone, Debug)]
pub struct DesktopSavePath {
    path: String,
}
fn string(path: &Path) -> io::Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path is not UTF-8"))
}
fn metadata(path: &str) -> io::Result<Option<fs::Metadata>> {
    match fs::metadata(path) {
        Ok(m) => Ok(Some(m)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
impl LocalSavePath for DesktopSavePath {
    type Error = io::Error;
    fn path(&self) -> &str {
        &self.path
    }
    fn name(&self) -> &str {
        Path::new(&self.path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
    }
    fn absolute_path(&self) -> io::Result<String> {
        let path = Path::new(&self.path);
        if path.is_absolute() {
            Ok(self.path.clone())
        } else {
            string(&std::env::current_dir()?.join(path))
        }
    }
    fn normalized_path(&self) -> io::Result<String> {
        string(&absolute(Path::new(&self.path))?)
    }
    fn ancestors(&self) -> io::Result<Vec<String>> {
        absolute(Path::new(&self.path))?
            .ancestors()
            .skip(1)
            .map(string)
            .collect()
    }
    fn is_file(&self) -> io::Result<bool> {
        Ok(metadata(&self.path)?.is_some_and(|m| m.is_file()))
    }
    fn is_directory(&self) -> io::Result<bool> {
        Ok(metadata(&self.path)?.is_some_and(|m| m.is_dir()))
    }
    fn exists(&self) -> io::Result<bool> {
        Ok(metadata(&self.path)?.is_some())
    }
    fn last_modified_ms(&self) -> io::Result<i64> {
        let Some(m) = metadata(&self.path)? else {
            return Ok(0);
        };
        let t = m.modified()?;
        match t.duration_since(UNIX_EPOCH) {
            Ok(d) => Ok(i64::try_from(d.as_millis()).map_err(io::Error::other)?),
            Err(e) => Ok(-i64::try_from(e.duration().as_millis()).map_err(io::Error::other)?),
        }
    }
    fn resolve(&self, relative: &str) -> io::Result<Self> {
        Ok(Self {
            path: string(&Path::new(&self.path).join(relative))?,
        })
    }
    fn resolve_remote(&self, name: &str) -> io::Result<Option<Self>> {
        resolve_remote_file(Path::new(&self.path), name)?
            .map(|p| string(&p).map(|path| Self { path }))
            .transpose()
    }
    fn walk_top_down(&self) -> io::Result<Vec<Self>> {
        let mut result = Vec::new();
        let mut todo = vec![(
            PathBuf::from(&self.path),
            std::sync::Arc::new(Vec::<PathBuf>::new()),
        )];
        while let Some((path, ancestors)) = todo.pop() {
            let metadata = match fs::metadata(&path) {
                Ok(m) => m,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };
            if metadata.is_dir() {
                let canonical = fs::canonicalize(&path)?;
                if ancestors.contains(&canonical) {
                    return Err(io::Error::other("cyclic save directory"));
                }
                let mut descendants = (*ancestors).clone();
                descendants.push(canonical);
                let descendants = std::sync::Arc::new(descendants);
                let entries = fs::read_dir(&path)?
                    .map(|e| e.map(|e| e.path()))
                    .collect::<io::Result<Vec<_>>>()?;
                todo.extend(
                    entries
                        .into_iter()
                        .rev()
                        .map(|p| (p, std::sync::Arc::clone(&descendants))),
                );
            }
            result.push(Self {
                path: string(&path)?,
            });
        }
        Ok(result)
    }
    fn relative_to(&self, root: &Self) -> io::Result<String> {
        let p = Path::new(&self.path)
            .strip_prefix(&root.path)
            .map_err(io::Error::other)?;
        Ok(string(p)?.replace('\\', "/"))
    }
}
impl LocalSaveAccess for DesktopSaveAccess {
    type Error = io::Error;
    type Path = DesktopSavePath;
    fn path(&self, path: &str) -> DesktopSavePath {
        DesktopSavePath {
            path: path.to_owned(),
        }
    }
    fn effective_targets(&self, entry: &GameEntry) -> io::Result<Vec<SaveTarget>> {
        LocalSaveTargets.for_entry(entry)
    }
    fn effective_targets_for_rom(
        &self,
        entry: &Emulator,
        rom: &str,
    ) -> io::Result<Vec<SaveTarget>> {
        LocalSaveTargets.for_rom(entry, rom)
    }
}
