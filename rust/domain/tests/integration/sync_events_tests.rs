use emusync_domain::sync::{RequestId, SaveConflict};

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
