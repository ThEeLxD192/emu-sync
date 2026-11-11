//! Cooperative cancellation remains distinct from recoverable failures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperationFailure<E> {
    Cancelled,
    Failed(E),
}

/// Recovery never converts cancellation into a successful operation.
pub fn recover<T, E>(
    result: Result<T, OperationFailure<E>>,
    recover: impl FnOnce(E) -> T,
) -> Result<T, OperationFailure<E>> {
    match result {
        Ok(value) => Ok(value),
        Err(OperationFailure::Failed(error)) => Ok(recover(error)),
        Err(OperationFailure::Cancelled) => Err(OperationFailure::Cancelled),
    }
}
