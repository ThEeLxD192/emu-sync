use emusync_application::{SaveTargetMapping, SaveTargetMappingError, SaveTargetResolver};
use emusync_domain::library::{EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType};
use emusync_domain::sync::{LocalSaveAccess, LocalSavePath, RemoteFile, SyncGame};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq)]
enum MockNode {
    File { modified_ms: i64 },
    Directory,
}

#[derive(Clone)]
struct MockFs {
    nodes: Arc<Mutex<HashMap<String, MockNode>>>,
}

impl MockFs {
    fn new() -> Self {
        Self {
            nodes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn add_file(&self, path: &str, modified_ms: i64) {
        let mut lock = self.nodes.lock().unwrap();
        lock.insert(normalize(path), MockNode::File { modified_ms });
        // ensure parents are directories
        let mut curr = normalize(path);
        while let Some((parent, _)) = curr.rsplit_once('/') {
            if parent.is_empty() {
                break;
            }
            lock.entry(parent.to_string())
                .or_insert(MockNode::Directory);
            curr = parent.to_string();
        }
    }

    fn add_dir(&self, path: &str) {
        let mut lock = self.nodes.lock().unwrap();
        lock.insert(normalize(path), MockNode::Directory);
    }
}

fn normalize(p: &str) -> String {
    let s = p.trim().replace('\\', "/");
    let mut parts = Vec::new();
    for seg in s.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            parts.pop();
        } else {
            parts.push(seg);
        }
    }
    let norm = parts.join("/");
    if s.starts_with('/') {
        format!("/{norm}")
    } else {
        norm
    }
}

#[derive(Clone)]
struct MockPath {
    fs: Arc<Mutex<HashMap<String, MockNode>>>,
    raw_path: String,
}

impl LocalSavePath for MockPath {
    type Error = String;

    fn path(&self) -> &str {
        &self.raw_path
    }

    fn name(&self) -> &str {
        self.raw_path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&self.raw_path)
    }

    fn absolute_path(&self) -> Result<String, Self::Error> {
        Ok(normalize(&self.raw_path))
    }

    fn normalized_path(&self) -> Result<String, Self::Error> {
        Ok(normalize(&self.raw_path))
    }

    fn ancestors(&self) -> Result<Vec<String>, Self::Error> {
        let norm = normalize(&self.raw_path);
        let mut result = Vec::new();
        let mut curr = norm.as_str();
        while let Some((parent, _)) = curr.rsplit_once('/') {
            if parent.is_empty() {
                result.push("/".to_string());
                break;
            }
            result.push(parent.to_string());
            curr = parent;
        }
        Ok(result)
    }

    fn is_file(&self) -> Result<bool, Self::Error> {
        let norm = normalize(&self.raw_path);
        let lock = self.fs.lock().unwrap();
        Ok(matches!(lock.get(&norm), Some(MockNode::File { .. })))
    }

    fn is_directory(&self) -> Result<bool, Self::Error> {
        let norm = normalize(&self.raw_path);
        let lock = self.fs.lock().unwrap();
        Ok(matches!(lock.get(&norm), Some(MockNode::Directory)))
    }

    fn exists(&self) -> Result<bool, Self::Error> {
        let norm = normalize(&self.raw_path);
        let lock = self.fs.lock().unwrap();
        Ok(lock.contains_key(&norm))
    }

    fn last_modified_ms(&self) -> Result<i64, Self::Error> {
        let norm = normalize(&self.raw_path);
        let lock = self.fs.lock().unwrap();
        match lock.get(&norm) {
            Some(MockNode::File { modified_ms }) => Ok(*modified_ms),
            _ => Ok(0),
        }
    }

    fn resolve(&self, relative: &str) -> Result<Self, Self::Error> {
        let norm_parent = normalize(&self.raw_path);
        let full = format!("{}/{}", norm_parent, relative.trim_start_matches('/'));
        Ok(MockPath {
            fs: Arc::clone(&self.fs),
            raw_path: full,
        })
    }

    fn resolve_remote(&self, name: &str) -> Result<Option<Self>, Self::Error> {
        if name
            .split('/')
            .any(|s| s == ".." || s == "." || s.is_empty())
        {
            return Ok(None);
        }
        let resolved = self.resolve(name)?;
        Ok(Some(resolved))
    }

    fn walk_top_down(&self) -> Result<Vec<Self>, Self::Error> {
        let norm_parent = normalize(&self.raw_path);
        let prefix = format!("{norm_parent}/");
        let lock = self.fs.lock().unwrap();
        let mut files = Vec::new();
        for key in lock.keys() {
            if key.starts_with(&prefix) {
                files.push(MockPath {
                    fs: Arc::clone(&self.fs),
                    raw_path: key.clone(),
                });
            }
        }
        files.sort_by(|a, b| a.raw_path.cmp(&b.raw_path));
        Ok(files)
    }

    fn relative_to(&self, root: &Self) -> Result<String, Self::Error> {
        let norm_self = normalize(&self.raw_path);
        let norm_root = normalize(&root.raw_path);
        let prefix = format!("{norm_root}/");
        if let Some(stripped) = norm_self.strip_prefix(&prefix) {
            Ok(stripped.to_string())
        } else if norm_self == norm_root {
            Ok(String::new())
        } else {
            Err(format!("{norm_self} is not relative to {norm_root}"))
        }
    }
}

impl LocalSaveAccess for MockFs {
    type Error = String;
    type Path = MockPath;

    fn path(&self, path: &str) -> Self::Path {
        MockPath {
            fs: Arc::clone(&self.nodes),
            raw_path: path.to_string(),
        }
    }

    fn effective_targets(&self, entry: &GameEntry) -> Result<Vec<SaveTarget>, Self::Error> {
        Ok(entry.common().save_targets.clone())
    }

    fn effective_targets_for_rom(
        &self,
        entry: &emusync_domain::library::Emulator,
        rom: &str,
    ) -> Result<Vec<SaveTarget>, Self::Error> {
        let targets = entry
            .save_targets_by_rom
            .iter()
            .find(|(r, _)| r == rom)
            .map(|(_, t)| t.clone())
            .unwrap_or_default();
        Ok(targets)
    }
}

#[test]
fn cloud_path_traversal_is_rejected() {
    let fs = MockFs::new();
    let mapping = SaveTargetMapping::new(fs);

    let targets = vec![SaveTarget {
        path: "/saves/slot1.sav".into(),
        kind: SaveTargetType::File,
        cloud_path: Some("../traversal/path".into()),
    }];

    let res = mapping.validate_save_target_mappings(&targets, &[]);
    assert!(matches!(
        res,
        Err(SaveTargetMappingError::InvalidCloudPath(_))
    ));
}

#[test]
fn conflicting_remote_to_local_paths_rejected() {
    let fs = MockFs::new();
    let mapping = SaveTargetMapping::new(fs.clone());

    // Directory target /saves
    fs.add_dir("/saves");
    let targets = vec![SaveTarget {
        path: "/saves".into(),
        kind: SaveTargetType::Directory,
        cloud_path: None,
    }];

    // Remote files: both 'file.sav' and 'subdir/../file.sav' or case colliding
    // If remote has 'save1' and 'save1/nested', ancestor collision occurs
    let cloud_files = vec![
        RemoteFile {
            id: "f1".into(),
            name: "save1".into(),
            modified: 1000,
            is_folder: false,
            sha256: None,
            version: None,
        },
        RemoteFile {
            id: "f2".into(),
            name: "save1/nested.sav".into(),
            modified: 1000,
            is_folder: false,
            sha256: None,
            version: None,
        },
    ];

    let res = mapping.validate_save_target_mappings(&targets, &cloud_files);
    assert!(matches!(
        res,
        Err(SaveTargetMappingError::ConflictingRemoteToLocalPaths)
    ));
}

#[test]
fn reserved_cloud_names_prevent_file_and_directory_name_collision() {
    let fs = MockFs::new();
    fs.add_file("/saves/custom.sav", 1000);
    fs.add_dir("/saves/dir");
    let mapping = SaveTargetMapping::new(fs);

    let file_target = SaveTarget {
        path: "/saves/custom.sav".into(),
        kind: SaveTargetType::File,
        cloud_path: None,
    };
    let dir_target = SaveTarget {
        path: "/saves/dir".into(),
        kind: SaveTargetType::Directory,
        cloud_path: None,
    };

    let siblings = vec![file_target.clone(), dir_target.clone()];
    let reserved = mapping
        .reserved_cloud_names(&dir_target, &siblings, &[])
        .unwrap();

    // The file target's cloud name "custom.sav" must be reserved so the directory doesn't overwrite it
    assert!(reserved.contains("custom.sav"));
}

#[test]
fn legacy_folders_ambiguity_detects_colliding_filenames_and_aborts() {
    let fs = MockFs::new();
    // Two legacy directories without cloud_path specified, both containing "state.dat"
    fs.add_file("/dir1/state.dat", 1000);
    fs.add_file("/dir2/state.dat", 2000);

    let mapping = SaveTargetMapping::new(fs);

    let legacy1 = SaveTarget {
        path: "/dir1".into(),
        kind: SaveTargetType::Directory,
        cloud_path: None,
    };
    let legacy2 = SaveTarget {
        path: "/dir2".into(),
        kind: SaveTargetType::Directory,
        cloud_path: None,
    };

    let res = mapping.validate_save_target_mappings(&[legacy1, legacy2], &[]);
    assert!(matches!(
        res,
        Err(SaveTargetMappingError::AmbiguousLegacyDirectories(_))
    ));
}

#[test]
fn target_resolver_resolves_cloud_path_segments() {
    let fs = MockFs::new();
    let resolver = SaveTargetResolver::new(fs);

    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Zelda".into(),
            cloud_folder: Some("CustomZeldaFolder".into()),
            ..EntryCommon::default()
        },
        wait_for_process: None,
        cover_path: None,
    });

    let sync_game = SyncGame {
        entry: entry.clone(),
        name: "Zelda".into(),
        effective_title: "The Legend of Zelda".into(),
        rom_path: None,
        save_targets: Vec::new(),
    };

    let segments = resolver.get_cloud_path_segments_game(&sync_game).unwrap();
    assert_eq!(segments, vec!["CustomZeldaFolder"]);
}

#[test]
fn conflicting_local_destinations_rejected() {
    let fs = MockFs::new();
    let mapping = SaveTargetMapping::new(fs);

    let targets = vec![
        SaveTarget {
            path: "/saves/slot1.sav".into(),
            kind: SaveTargetType::File,
            cloud_path: Some("cloud1.sav".into()),
        },
        SaveTarget {
            path: "/saves/slot1.sav".into(),
            kind: SaveTargetType::File,
            cloud_path: Some("cloud2.sav".into()),
        },
    ];

    let res = mapping.validate_save_target_mappings(&targets, &[]);
    assert_eq!(
        res,
        Err(SaveTargetMappingError::ConflictingLocalDestinations)
    );
}

#[test]
fn file_is_parent_of_another_rejected() {
    let fs = MockFs::new();
    fs.add_file("/saves/parent.sav", 1000);
    fs.add_file("/saves/parent.sav/child.sav", 1000);
    let mapping = SaveTargetMapping::new(fs);

    let targets = vec![
        SaveTarget {
            path: "/saves/parent.sav".into(),
            kind: SaveTargetType::File,
            cloud_path: None,
        },
        SaveTarget {
            path: "/saves/parent.sav/child.sav".into(),
            kind: SaveTargetType::File,
            cloud_path: None,
        },
    ];

    let res = mapping.validate_save_target_mappings(&targets, &[]);
    assert_eq!(res, Err(SaveTargetMappingError::FileIsParentOfAnother));
}

#[test]
fn overlapping_directory_scopes_rejected() {
    let fs = MockFs::new();
    fs.add_dir("/saves/dir1");
    fs.add_dir("/saves/dir2");
    let mapping = SaveTargetMapping::new(fs);

    let targets = vec![
        SaveTarget {
            path: "/saves/dir1".into(),
            kind: SaveTargetType::Directory,
            cloud_path: Some("common".into()),
        },
        SaveTarget {
            path: "/saves/dir2".into(),
            kind: SaveTargetType::Directory,
            cloud_path: Some("common/sub".into()),
        },
    ];

    let res = mapping.validate_save_target_mappings(&targets, &[]);
    assert_eq!(res, Err(SaveTargetMappingError::OverlappingDirectoryScopes));
}

#[test]
fn project_directory_files_strips_prefix_and_filters() {
    let files = vec![
        RemoteFile {
            id: "1".into(),
            name: "sub/save1.dat".into(),
            modified: 1000,
            is_folder: false,
            sha256: None,
            version: None,
        },
        RemoteFile {
            id: "2".into(),
            name: "other/save2.dat".into(),
            modified: 1000,
            is_folder: false,
            sha256: None,
            version: None,
        },
    ];

    let projected = emusync_application::project_directory_files(&files, Some("sub"));
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].name, "save1.dat");

    let all_files = emusync_application::project_directory_files(&files, None);
    assert_eq!(all_files.len(), 2);
}

#[test]
fn target_resolver_get_save_targets_for_native_and_emulator() {
    let fs = MockFs::new();
    let resolver = SaveTargetResolver::new(fs);

    // 1. NativeGame
    let native = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "NativeGame".into(),
            save_targets: vec![SaveTarget {
                path: "/saves/native.sav".into(),
                kind: SaveTargetType::File,
                cloud_path: None,
            }],
            ..Default::default()
        },
        ..Default::default()
    });

    let targets = resolver.get_save_targets(&native).unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].display_name, "NativeGame");
    assert_eq!(targets[0].file.path(), "/saves/native.sav");
    assert_eq!(targets[0].cloud_path_segments, vec!["NativeGame"]);

    // 2. Emulator with global and per-ROM targets, including a duplicate
    let emu = GameEntry::Emulator(emusync_domain::library::Emulator {
        common: EntryCommon {
            name: "RetroArch".into(),
            cloud_folder: Some("CloudEmu".into()),
            save_targets: vec![SaveTarget {
                path: "/saves/global.sav".into(),
                kind: SaveTargetType::File,
                cloud_path: None,
            }],
            ..Default::default()
        },
        save_targets_by_rom: vec![
            (
                "game1.sfc".into(),
                vec![SaveTarget {
                    path: "/saves/game1.srm".into(),
                    kind: SaveTargetType::File,
                    cloud_path: None,
                }],
            ),
            (
                "game2.sfc".into(),
                vec![
                    // duplicate of global.sav
                    SaveTarget {
                        path: "/saves/global.sav".into(),
                        kind: SaveTargetType::File,
                        cloud_path: None,
                    },
                ],
            ),
        ],
        ..Default::default()
    });

    let emu_targets = resolver.get_save_targets(&emu).unwrap();
    // /saves/global.sav should only appear once due to deduplication
    assert_eq!(emu_targets.len(), 2);
    assert!(
        emu_targets
            .iter()
            .any(|t| t.file.path() == "/saves/global.sav")
    );
    assert!(
        emu_targets
            .iter()
            .any(|t| t.file.path() == "/saves/game1.srm")
    );
}

#[test]
fn resolved_save_target_configured_preserves_target_properties() {
    let fs = MockFs::new();
    let resolver = SaveTargetResolver::new(fs);
    let native = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Game".into(),
            save_targets: vec![SaveTarget {
                path: "/saves/game.sav".into(),
                kind: SaveTargetType::File,
                cloud_path: Some("custom.sav".into()),
            }],
            ..Default::default()
        },
        ..Default::default()
    });

    let targets = resolver.get_save_targets(&native).unwrap();
    assert_eq!(targets.len(), 1);
    let configured = targets[0].configured();
    assert_eq!(configured.path, "/saves/game.sav");
    assert_eq!(configured.kind, SaveTargetType::File);
    assert_eq!(configured.cloud_path, Some("custom.sav".into()));
}
