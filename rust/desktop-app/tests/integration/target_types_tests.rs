//! Save target kinds are corrected with the filesystem before persisting, as Kotlin's
//! `SaveTargetFormState.buildTargets` does.

use emusync_data_local::LocalSaveTargets;
use emusync_desktop_app::target_types::{resolve_entry_target_kinds, resolve_target_kinds};
use emusync_domain::library::{EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType};
use std::fs;
use tempfile::tempdir;

fn target(path: &std::path::Path, kind: SaveTargetType) -> SaveTarget {
    SaveTarget {
        path: path.to_string_lossy().into_owned(),
        kind,
        cloud_path: Some("cloud/name".to_string()),
        excluded_folders: Vec::new(),
    }
}

#[test]
fn existing_paths_take_their_real_kind_and_missing_paths_keep_the_chosen_one() {
    let dir = tempdir().unwrap();
    let folder = dir.path().join("saves");
    let file = dir.path().join("slot.sav");
    fs::create_dir(&folder).unwrap();
    fs::write(&file, b"save").unwrap();
    let missing = dir.path().join("missing");

    let resolved = resolve_target_kinds(
        vec![
            target(&folder, SaveTargetType::File),
            target(&file, SaveTargetType::Directory),
            target(&missing, SaveTargetType::Directory),
            target(&missing, SaveTargetType::Unknown),
        ],
        &LocalSaveTargets,
    );

    let kinds: Vec<_> = resolved.iter().map(|t| t.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![
            SaveTargetType::Directory,
            SaveTargetType::File,
            SaveTargetType::Directory,
            SaveTargetType::Unknown,
        ]
    );
    assert!(
        resolved
            .iter()
            .all(|t| t.cloud_path.as_deref() == Some("cloud/name")),
        "only the kind changes"
    );
}

#[test]
fn entry_targets_are_resolved_and_other_fields_are_untouched() {
    let dir = tempdir().unwrap();
    let folder = dir.path().join("saves");
    fs::create_dir(&folder).unwrap();
    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Hollow Knight".to_string(),
            save_paths: vec![folder.to_string_lossy().into_owned()],
            save_targets: vec![target(&folder, SaveTargetType::File)],
            ..EntryCommon::default()
        },
        ..NativeGame::default()
    });

    let resolved = resolve_entry_target_kinds(entry.clone(), &LocalSaveTargets);

    assert_eq!(
        resolved.common().save_targets[0].kind,
        SaveTargetType::Directory
    );
    assert_eq!(resolved.common().name, entry.common().name);
    assert_eq!(resolved.common().save_paths, entry.common().save_paths);
}
