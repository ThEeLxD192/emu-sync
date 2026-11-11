use emusync_application::{SaveTargetMapping, SaveTargetMappingError};
use emusync_domain::library::{GameEntry, SaveTarget, SaveTargetType};
use emusync_domain::sync::{LocalSaveAccess, LocalSavePath, RemoteFile};
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

    let projected = emusync_application::sync::project_directory_files(&files, Some("sub"));
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].name, "save1.dat");

    let all_files = emusync_application::sync::project_directory_files(&files, None);
    assert_eq!(all_files.len(), 2);
}

#[test]
fn sibling_legacy_directories_reserve_newer_files_and_report_all_ambiguity_categories() {
    // Subcase A: newer_sibling_paths reserves file for dir_b, but not for dir_a
    {
        let fs = MockFs::new();
        fs.add_dir("/saves/dir_a");
        fs.add_file("/saves/dir_a/slot1.sav", 5000);
        fs.add_dir("/saves/dir_b");
        fs.add_file("/saves/dir_b/slot1.sav", 1000);

        let target_a = SaveTarget {
            path: "/saves/dir_a".into(),
            kind: SaveTargetType::Directory,
            cloud_path: None,
        };
        let target_b = SaveTarget {
            path: "/saves/dir_b".into(),
            kind: SaveTargetType::Directory,
            cloud_path: None,
        };
        let all_targets = vec![target_a.clone(), target_b.clone()];

        let mapping = SaveTargetMapping::new(fs);

        // When inspecting dir_b, dir_a has a newer slot1.sav, so it must be reserved
        let reserved_b = mapping
            .reserved_cloud_names(&target_b, &all_targets, &[])
            .unwrap();
        assert!(
            reserved_b.contains("slot1.sav"),
            "slot1.sav must be reserved for dir_b because dir_a has a newer version"
        );

        // When inspecting dir_a, dir_b has an older slot1.sav, so it must NOT be reserved
        let reserved_a = mapping
            .reserved_cloud_names(&target_a, &all_targets, &[])
            .unwrap();
        assert!(
            !reserved_a.contains("slot1.sav"),
            "slot1.sav must not be reserved for dir_a because dir_a itself has the newer version"
        );
    }

    // Subcase B: AmbiguousLegacyDirectories reports all three categories
    {
        let fs = MockFs::new();
        // 1. dir1 and dir2 both exist and share colliding.sav -> "Names found in multiple save folders"
        fs.add_dir("/saves/dir1");
        fs.add_file("/saves/dir1/colliding.sav", 1000);
        fs.add_dir("/saves/dir2");
        fs.add_file("/saves/dir2/colliding.sav", 1000);

        // missing_dir does NOT exist on disk -> "Folders unavailable on this device"
        let targets = vec![
            SaveTarget {
                path: "/saves/missing_dir".into(),
                kind: SaveTargetType::Directory,
                cloud_path: None,
            },
            SaveTarget {
                path: "/saves/dir1".into(),
                kind: SaveTargetType::Directory,
                cloud_path: None,
            },
            SaveTarget {
                path: "/saves/dir2".into(),
                kind: SaveTargetType::Directory,
                cloud_path: None,
            },
        ];

        // Cloud files contain colliding.sav AND unmatched_remote.sav -> "Cloud files with no matching local save"
        let cloud_files = vec![
            RemoteFile {
                id: "rf_col".into(),
                name: "colliding.sav".into(),
                modified: 1000,
                is_folder: false,
                sha256: None,
                version: None,
            },
            RemoteFile {
                id: "rf_unmatched".into(),
                name: "unmatched_remote.sav".into(),
                modified: 1000,
                is_folder: false,
                sha256: None,
                version: None,
            },
        ];

        let mapping = SaveTargetMapping::new(fs);
        let err = mapping
            .validate_save_target_mappings(&targets, &cloud_files)
            .expect_err("Expected AmbiguousLegacyDirectories error");

        match err {
            SaveTargetMappingError::AmbiguousLegacyDirectories(msg) => {
                assert!(
                    msg.contains("Folders unavailable on this device"),
                    "Error must contain 'Folders unavailable on this device', got:\n{msg}"
                );
                assert!(
                    msg.contains("Names found in multiple save folders"),
                    "Error must contain 'Names found in multiple save folders', got:\n{msg}"
                );
                assert!(
                    msg.contains("Cloud files with no matching local save"),
                    "Error must contain 'Cloud files with no matching local save', got:\n{msg}"
                );
            }
            other => panic!("Expected AmbiguousLegacyDirectories, got: {other:?}"),
        }
    }
}

fn cloud_file(name: &str) -> RemoteFile {
    RemoteFile {
        id: format!("id-{name}"),
        name: name.into(),
        modified: 1000,
        is_folder: false,
        sha256: None,
        version: None,
    }
}

fn legacy_dir(path: &str) -> SaveTarget {
    SaveTarget {
        path: path.into(),
        kind: SaveTargetType::Directory,
        cloud_path: None,
    }
}

fn unplaced(
    mapping: &SaveTargetMapping<MockFs>,
    targets: &[SaveTarget],
    cloud: &[RemoteFile],
) -> emusync_application::UnplacedLegacyFiles {
    match mapping.validate_save_target_mappings(targets, cloud) {
        Err(SaveTargetMappingError::UnplacedLegacyFiles(unplaced)) => unplaced,
        other => panic!("expected cloud files to place, got {other:?}"),
    }
}

#[test]
fn new_device_with_empty_legacy_folders_gets_files_to_place_instead_of_a_dead_end() {
    // Steam Deck case: both shared-root folders exist but are empty on this device.
    let fs = MockFs::new();
    fs.add_dir("/sd/Saves/savedata/1/CUSA13795");
    fs.add_dir("/sd/Saves/trophies/CUSA13795");
    let targets = [
        legacy_dir("/sd/Saves/savedata/1/CUSA13795"),
        legacy_dir("/sd/Saves/trophies/CUSA13795"),
    ];
    let cloud = [
        cloud_file("savedata1/SYS-DATA.sav"),
        cloud_file("savedata1.sce_sys/param.bin"),
        cloud_file("savedata1.sce_sys/icon0.png"),
        cloud_file("TROPHY.TRP"),
    ];
    let mapping = SaveTargetMapping::new(fs);

    let unplaced = unplaced(&mapping, &targets, &cloud);

    assert_eq!(
        unplaced.folders,
        vec![
            "/sd/Saves/savedata/1/CUSA13795".to_string(),
            "/sd/Saves/trophies/CUSA13795".to_string()
        ]
    );
    assert!(unplaced.inferred.is_empty());
    let groups: Vec<_> = unplaced
        .groups
        .iter()
        .map(|g| (g.name.as_str(), g.files.len(), g.suggested_folder))
        .collect();
    // Every file is offered exactly once, grouped by its first segment, with the
    // folder whose name resembles the group preselected.
    assert_eq!(
        groups,
        vec![
            ("TROPHY.TRP", 1, Some(1)),
            ("savedata1", 1, Some(0)),
            ("savedata1.sce_sys", 2, Some(0)),
        ]
    );
    // Callers that cannot ask still explain the situation.
    let message = SaveTargetMappingError::UnplacedLegacyFiles(unplaced).to_string();
    assert!(message.contains("Cloud files with no matching local save"));
    assert!(message.contains("savedata1.sce_sys/icon0.png"));
}

#[test]
fn a_parent_directory_held_by_one_folder_places_the_file_without_asking() {
    let fs = MockFs::new();
    fs.add_dir("/saves/a");
    fs.add_dir("/saves/b");
    fs.add_dir("/saves/a/slot1");
    fs.add_dir("/saves/a/shared");
    fs.add_dir("/saves/b/shared");
    let targets = [legacy_dir("/saves/a"), legacy_dir("/saves/b")];
    let cloud = [
        cloud_file("slot1/new.sav"),
        cloud_file("shared/x.sav"),
        cloud_file("loose.sav"),
    ];
    let mapping = SaveTargetMapping::new(fs);

    let unplaced = unplaced(&mapping, &targets, &cloud);

    assert_eq!(unplaced.inferred, vec![("slot1/new.sav".to_string(), 0)]);
    // A parent present in both folders, or none, decides nothing.
    let asked: Vec<_> = unplaced.groups.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(asked, vec!["loose.sav", "shared"]);
    assert!(unplaced.groups.iter().all(|g| g.suggested_folder.is_none()));
}

#[test]
fn files_already_in_a_folder_need_no_placement() {
    let fs = MockFs::new();
    fs.add_file("/saves/a/one.sav", 1000);
    fs.add_dir("/saves/b");
    let targets = [legacy_dir("/saves/a"), legacy_dir("/saves/b")];
    let mapping = SaveTargetMapping::new(fs);

    assert_eq!(
        mapping.validate_save_target_mappings(&targets, &[cloud_file("one.sav")]),
        Ok(())
    );
}

#[test]
fn an_unavailable_legacy_folder_still_blocks_instead_of_offering_placement() {
    // A missing folder may be an unmounted SD card: never restore around it.
    let fs = MockFs::new();
    fs.add_dir("/saves/a");
    let targets = [legacy_dir("/saves/a"), legacy_dir("/media/sd/b")];
    let mapping = SaveTargetMapping::new(fs);

    let result = mapping.validate_save_target_mappings(&targets, &[cloud_file("one.sav")]);

    assert!(
        matches!(&result, Err(SaveTargetMappingError::AmbiguousLegacyDirectories(msg)) if msg.contains("Folders unavailable on this device")),
        "got {result:?}"
    );
}
