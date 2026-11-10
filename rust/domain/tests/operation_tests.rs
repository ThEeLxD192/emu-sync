use emusync_domain::operation::{OperationFailure, recover};

#[test]
fn cancellation_is_not_recovered_as_a_normal_failure() {
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
fn success_passes_through_without_invoking_recovery() {
    assert_eq!(
        recover::<u8, &str>(Ok(7), |_| panic!("recovery must not run")),
        Ok(7)
    );
}
