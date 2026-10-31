package com.emusync.drive

import kotlinx.coroutines.CancellationException

/**
 * Recovers transport, storage and decoding failures at Drive operation boundaries. Cancellation
 * always escapes before recovery; recovery may preserve a domain failure unchanged. Inline actions
 * retain the caller's suspend context and non-local returns.
 */
// These boundaries intentionally recover failures from several independent IO implementations.
@Suppress("TooGenericExceptionCaught")
internal inline fun <T> recoverDriveFailure(action: () -> T, recover: (Exception) -> T): T =
    try {
        action()
    } catch (cancelled: CancellationException) {
        throw cancelled
    } catch (failure: Exception) {
        recover(failure)
    }
