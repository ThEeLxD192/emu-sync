use emusync_data_local::{path_identity, relative_within, same_path};
use std::path::{Path, PathBuf};

#[test]
fn spellings_differing_only_in_case_share_an_identity_where_case_is_ignored() {
    let upper = Path::new("/Saves/Game/Slot1.DAT");
    let lower = Path::new("/saves/game/slot1.dat");
    assert!(same_path(upper, lower, true).unwrap());
    assert!(!same_path(upper, lower, false).unwrap());
    // Case folding must not merge genuinely different files.
    assert!(!same_path(upper, Path::new("/saves/game/slot2.dat"), true).unwrap());
    assert!(!same_path(upper, Path::new("/saves/game"), true).unwrap());
}

#[test]
fn identity_collapses_dot_segments_before_folding() {
    let messy = Path::new("/Saves/./Other/../Game/slot.dat");
    assert_eq!(
        path_identity(messy, true).unwrap(),
        PathBuf::from("/SAVES/GAME/SLOT.DAT")
    );
    assert_eq!(
        path_identity(messy, false).unwrap(),
        PathBuf::from("/Saves/Game/slot.dat")
    );
}

#[test]
fn relative_paths_keep_the_file_spelling_and_respect_component_boundaries() {
    let file = Path::new("/Saves/Game/Sub/Slot1.DAT");
    assert_eq!(
        relative_within(file, Path::new("/saves/game"), true),
        Some(PathBuf::from("Sub/Slot1.DAT"))
    );
    assert_eq!(relative_within(file, Path::new("/saves/game"), false), None);
    assert_eq!(
        relative_within(file, Path::new("/Saves/Game"), false),
        Some(PathBuf::from("Sub/Slot1.DAT"))
    );
    // A shared string prefix is not a shared directory.
    assert_eq!(
        relative_within(Path::new("/saves2/x.dat"), Path::new("/saves"), true),
        None
    );
    // The root itself is relative to itself, but a longer root is never a prefix.
    assert_eq!(
        relative_within(Path::new("/saves"), Path::new("/SAVES"), true),
        Some(PathBuf::new())
    );
    assert_eq!(
        relative_within(Path::new("/saves"), Path::new("/saves/game"), true),
        None
    );
}
