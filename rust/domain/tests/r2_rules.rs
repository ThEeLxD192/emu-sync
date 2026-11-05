use emusync_domain::{
    execution::{format_arguments, parse_arguments},
    sync::*,
};

#[test]
fn arguments_match_literal_examples() {
    let args = vec![
        "--profile".into(),
        "My Profile".into(),
        "".into(),
        "a\"b".into(),
        "C:\\Games\\".into(),
    ];
    assert_eq!(
        format_arguments(&args),
        "--profile \"My Profile\" \"\" \"a\\\"b\" \"C:\\\\Games\\\\\""
    );
    assert_eq!(
        parse_arguments("--profile \"My Profile\" \"\" \"a\\\"b\" \"C:\\\\Games\\\\\""),
        Some(args)
    );
    assert_eq!(
        parse_arguments("\"C:\\Games\\My Game\""),
        Some(vec!["C:\\Games\\My Game".into()])
    );
    assert_eq!(
        parse_arguments("prefix\" with spaces \"suffix"),
        Some(vec!["prefix with spaces suffix".into()])
    );
    assert_eq!(parse_arguments("\"unfinished\\"), None);
    assert_eq!(parse_arguments("\"unfinished"), None);
}

#[test]
fn paths_reject_absolute_and_traversal_names() {
    for invalid in [
        "", " x", "x ", "/save", "\\save", "C:save", "a//b", "a/../b", "a/./b", "a/", "a\0b",
    ] {
        assert_eq!(remote_path_segments(invalid), None, "{invalid:?}");
    }
    assert_eq!(
        remote_path_segments("slot\\sub/save.dat"),
        Some(vec!["slot", "sub", "save.dat"])
    );
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
fn cancellation_is_not_recovered_as_a_normal_failure() {
    use emusync_domain::operation::{OperationFailure, recover};
    assert_eq!(
        recover::<_, &str>(Err(OperationFailure::Failed("original")), |e| e.len()),
        Ok(8)
    );
    assert_eq!(
        recover::<usize, &str>(Err(OperationFailure::Cancelled), |_| panic!(
            "cancellation must escape"
        )),
        Err(OperationFailure::Cancelled)
    );
}

#[test]
fn arguments_preserve_non_special_escapes_and_format_literals() {
    assert_eq!(
        parse_arguments("\"a\\\"b\" \"a\\\\b\" \"a\\qb\""),
        Some(vec!["a\"b".into(), "a\\b".into(), "a\\qb".into()])
    );
    assert_eq!(
        format_arguments(&["plain".into(), "a\tb".into(), "a\\b".into()]),
        "plain \"a\tb\" \"a\\\\b\""
    );
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
fn equal_looking_conflict_requests_keep_distinct_identity() {
    let first = SaveConflict {
        request_id: RequestId(1),
        local_modified_ms: 1,
        cloud_modified_ms: 2,
        game_name: "game".into(),
    };
    let second = SaveConflict {
        request_id: RequestId(2),
        ..first.clone()
    };
    assert_ne!(first, second);
}
