use emusync_domain::sync::{
    Checkpoint, CloudFile, Decision, LocalFile, LocalNewerAction, RemoteState, plan_directory,
    plan_sync, plan_tracked,
};

/// One second in nanoseconds.
const S: i128 = 1_000_000_000;

fn local_file(path: &str, modified: i128) -> LocalFile {
    LocalFile {
        path: path.into(),
        modified,
    }
}

fn cloud_file(id: &str, path: &str, modified: i128) -> CloudFile {
    CloudFile {
        id: id.into(),
        path: path.into(),
        modified,
    }
}

#[test]
fn initial_decisions_preserve_tolerance_and_post_play_rule() {
    use Decision::*;
    use LocalNewerAction::*;
    for (local, cloud, action, expected) in [
        (None, None, ResolveConflict, InSync),
        (Some(0), None, ResolveConflict, UploadLocal),
        (None, Some(0), ResolveConflict, DownloadCloud),
        (Some(5_000_000_000), Some(0), ResolveConflict, InSync),
        (Some(5_001_000_000), Some(0), ResolveConflict, Conflict),
        (Some(0), Some(5_900_000_000), ResolveConflict, DownloadCloud),
        (Some(1_000_000), Some(0), Upload, UploadLocal),
    ] {
        assert_eq!(plan_sync(local, cloud, 5, action), expected);
    }
}

#[test]
fn tracked_decisions_use_content_and_remote_identity() {
    use Decision::*;
    let mut remote = RemoteState {
        id: "id".into(),
        modified: 0,
        sha256: Some("OLD".into()),
        version: Some("1".into()),
    };
    let previous = Checkpoint {
        local_sha256: "old".into(),
        remote_id: "id".into(),
        remote_modified: 0,
        remote_sha256: Some("old".into()),
        remote_version: Some("1".into()),
    };
    assert_eq!(plan_tracked(Some("old"), Some(&remote), None), InSync);
    assert_eq!(plan_tracked(Some("new"), Some(&remote), None), Conflict);
    assert_eq!(
        plan_tracked(Some("new"), Some(&remote), Some(&previous)),
        UploadLocal
    );
    remote.sha256 = Some("remote".into());
    assert_eq!(
        plan_tracked(Some("old"), Some(&remote), Some(&previous)),
        DownloadCloud
    );
    assert_eq!(
        plan_tracked(Some("new"), Some(&remote), Some(&previous)),
        Conflict
    );
    remote.sha256 = Some("NEW".into());
    assert_eq!(
        plan_tracked(Some("new"), Some(&remote), Some(&previous)),
        InSync
    );
    remote.sha256 = None;
    remote.version = Some("2".into());
    assert_eq!(
        plan_tracked(Some("old"), Some(&remote), Some(&previous)),
        DownloadCloud
    );
    remote.version = None;
    remote.modified = 1;
    assert_eq!(
        plan_tracked(Some("old"), Some(&remote), Some(&previous)),
        DownloadCloud
    );
    remote.modified = 0;
    remote.id = "replacement".into();
    assert_eq!(
        plan_tracked(Some("new"), Some(&remote), Some(&previous)),
        Conflict
    );
    assert_eq!(
        plan_tracked(None, Some(&remote), Some(&previous)),
        DownloadCloud
    );
    assert_eq!(
        plan_tracked(Some("old"), None, Some(&previous)),
        UploadLocal
    );
    assert_eq!(plan_tracked(None, None, None), InSync);
}

#[test]
fn directory_plan_sorts_and_retains_older_duplicates() {
    let local = vec![
        LocalFile {
            path: "shared".into(),
            modified: 10_000_000_000,
        },
        LocalFile {
            path: "local".into(),
            modified: 0,
        },
    ];
    let cloud = vec![
        CloudFile {
            id: "b".into(),
            path: "shared".into(),
            modified: 0,
        },
        CloudFile {
            id: "a".into(),
            path: "shared".into(),
            modified: 0,
        },
        CloudFile {
            id: "c".into(),
            path: "cloud".into(),
            modified: 0,
        },
    ];
    let result = plan_directory(&local, &cloud, LocalNewerAction::ResolveConflict);
    assert_eq!(
        result
            .iter()
            .map(|f| (f.path.as_str(), f.decision))
            .collect::<Vec<_>>(),
        vec![
            ("cloud", Decision::DownloadCloud),
            ("local", Decision::UploadLocal),
            ("shared", Decision::Conflict)
        ]
    );
    assert_eq!(result[2].cloud.as_ref().unwrap().id, "b");
    assert_eq!(result[2].duplicates[0].id, "a");
}

#[test]
fn directory_order_matches_kotlin_utf16_order() {
    let files = vec![
        LocalFile {
            path: "\u{e000}".into(),
            modified: 0,
        },
        LocalFile {
            path: "\u{10000}".into(),
            modified: 0,
        },
    ];
    let result = plan_directory(&files, &[], LocalNewerAction::ResolveConflict);
    assert_eq!(
        result.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        ["\u{10000}", "\u{e000}"]
    );
}

#[test]
fn post_play_upload_never_overrides_newer_or_equal_cloud_copy() {
    use Decision::*;
    for (local, cloud, expected) in [
        (0, 6 * S, DownloadCloud),
        (0, 0, InSync),
        (0, 3 * S, InSync),
        (3600 * S, 0, UploadLocal),
        (1_000_000, 1_999_999, InSync),
        (1_999_999, 1_000_000, InSync),
        (2_000_000, 1_999_999, UploadLocal),
    ] {
        assert_eq!(
            plan_sync(Some(local), Some(cloud), 5, LocalNewerAction::Upload),
            expected,
            "local {local} cloud {cloud}"
        );
    }
}

#[test]
fn tolerance_boundary_is_exact_to_the_nanosecond_in_both_directions() {
    use Decision::*;
    for (local, cloud, threshold, expected) in [
        (5 * S + 1, 0, 5, Conflict),
        (0, 5 * S, 5, InSync),
        (0, 5 * S + 1, 5, DownloadCloud),
        (7, 7, 0, InSync),
        (8, 7, 0, Conflict),
        (7, 8, 0, DownloadCloud),
    ] {
        assert_eq!(
            plan_sync(
                Some(local),
                Some(cloud),
                threshold,
                LocalNewerAction::ResolveConflict
            ),
            expected,
            "local {local} cloud {cloud} threshold {threshold}"
        );
    }
}

#[test]
fn extreme_timestamps_and_thresholds_do_not_overflow() {
    use Decision::*;
    use LocalNewerAction::*;
    for (local, cloud, threshold, action, expected) in [
        (i128::MAX, i128::MIN, u64::MAX, ResolveConflict, Conflict),
        (
            i128::MIN,
            i128::MAX,
            u64::MAX,
            ResolveConflict,
            DownloadCloud,
        ),
        (i128::MAX, i128::MIN, u64::MAX, Upload, UploadLocal),
        (i128::MIN, i128::MAX, 0, Upload, DownloadCloud),
    ] {
        assert_eq!(
            plan_sync(Some(local), Some(cloud), threshold, action),
            expected,
            "local {local} cloud {cloud} threshold {threshold} {action:?}"
        );
    }
}

#[test]
fn tracked_bootstrap_without_remote_hash_is_a_conflict() {
    let remote = RemoteState {
        id: "id".into(),
        modified: 0,
        sha256: None,
        version: Some("1".into()),
    };
    assert_eq!(
        plan_tracked(Some("old"), Some(&remote), None),
        Decision::Conflict
    );
}

#[test]
fn tracked_remote_change_uses_hash_then_version_then_timestamp() {
    use Decision::*;
    let remote = |id: &str, modified: i128, sha: Option<&str>, version: Option<&str>| RemoteState {
        id: id.into(),
        modified,
        sha256: sha.map(str::to_owned),
        version: version.map(str::to_owned),
    };
    let checkpoint = |remote_sha: Option<&str>, remote_version: Option<&str>| Checkpoint {
        local_sha256: "old".into(),
        remote_id: "id".into(),
        remote_modified: 0,
        remote_sha256: remote_sha.map(str::to_owned),
        remote_version: remote_version.map(str::to_owned),
    };
    let full = checkpoint(Some("old"), Some("1"));
    for (local, current, previous, expected) in [
        (
            "new",
            remote("id", 0, Some("OLD"), Some("1")),
            checkpoint(None, Some("1")),
            UploadLocal,
        ),
        (
            "old",
            remote("id", 0, Some("old2"), Some("1")),
            checkpoint(None, Some("1")),
            DownloadCloud,
        ),
        (
            "old",
            remote("id", 0, None, Some("2")),
            checkpoint(None, None),
            InSync,
        ),
        (
            "old",
            remote("id", 1, None, Some("2")),
            checkpoint(None, None),
            DownloadCloud,
        ),
        (
            "old",
            remote("id", 0, None, Some("1")),
            full.clone(),
            InSync,
        ),
        (
            "new",
            remote("id", 99, Some("old"), Some("2")),
            full.clone(),
            UploadLocal,
        ),
        (
            "new",
            remote("id", 5, None, Some("1")),
            full.clone(),
            UploadLocal,
        ),
        (
            "old",
            remote("replacement", 0, None, Some("1")),
            full.clone(),
            DownloadCloud,
        ),
        (
            "new",
            remote("replacement", 0, Some("NEW"), None),
            full.clone(),
            InSync,
        ),
    ] {
        assert_eq!(
            plan_tracked(Some(local), Some(&current), Some(&previous)),
            expected,
            "local {local} remote {current:?} previous {previous:?}"
        );
    }
}

#[test]
fn directory_plan_picks_newest_duplicate_regardless_of_input_order() {
    let local = [local_file("save", 10 * S)];
    let cloud = [
        cloud_file("z", "save", 0),
        cloud_file("a", "save", 8 * S),
        cloud_file("m", "save", 3 * S),
        cloud_file("\u{1F600}", "save", 8 * S - 1),
    ];
    for order in [[0, 1, 2, 3], [3, 2, 1, 0], [1, 3, 0, 2]] {
        let ordered: Vec<_> = order.iter().map(|&i| cloud[i].clone()).collect();
        let result = plan_directory(&local, &ordered, LocalNewerAction::ResolveConflict);
        assert_eq!(result.len(), 1, "order {order:?}");
        assert_eq!(result[0].decision, Decision::InSync, "order {order:?}");
        assert_eq!(result[0].cloud.as_ref().unwrap().id, "a", "order {order:?}");
        let duplicates: Vec<_> = result[0].duplicates.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(duplicates, ["z", "m", "\u{1F600}"], "order {order:?}");
    }
    // U+FF5E sorts after U+1F600 in UTF-16, so it is the newest of two equal timestamps.
    let tied = [
        cloud_file("\u{FF5E}", "save", 0),
        cloud_file("\u{1F600}", "save", 0),
    ];
    let result = plan_directory(&[], &tied, LocalNewerAction::ResolveConflict);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].cloud.as_ref().unwrap().id, "\u{FF5E}");
    let duplicates: Vec<_> = result[0].duplicates.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(duplicates, ["\u{1F600}"]);
    assert_eq!(result[0].decision, Decision::DownloadCloud);
}

#[test]
fn directory_plan_applies_post_play_action_and_accepts_empty_inputs() {
    let local = [local_file("save", 1_000_000)];
    let cloud = [cloud_file("id", "save", 0)];
    let upload = plan_directory(&local, &cloud, LocalNewerAction::Upload);
    assert_eq!(upload.len(), 1);
    assert_eq!(upload[0].decision, Decision::UploadLocal);
    let resolve = plan_directory(&local, &cloud, LocalNewerAction::ResolveConflict);
    assert_eq!(resolve.len(), 1);
    assert_eq!(resolve[0].decision, Decision::InSync);
    assert!(plan_directory(&[], &[], LocalNewerAction::Upload).is_empty());
}
