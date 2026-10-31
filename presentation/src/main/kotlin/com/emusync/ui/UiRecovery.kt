package com.emusync.ui

import kotlinx.coroutines.CancellationException

/** Reports independent transport, storage and desktop failures without consuming cancellation. */
// UI actions expose one error result for failures from several independent IO implementations.
@Suppress("TooGenericExceptionCaught")
internal inline fun <T> recoverUiFailure(action: () -> T, recover: (Exception) -> T): T =
    try {
        action()
    } catch (cancelled: CancellationException) {
        throw cancelled
    } catch (failure: Exception) {
        recover(failure)
    }

/** Restores transient state before cancellation reaches the caller, retaining the caller's job. */
internal inline fun <T> restoreUiOnCancellation(action: () -> T, restore: () -> Unit): T {
    var cancelled = false
    try {
        return action()
    } catch (failure: CancellationException) {
        cancelled = true
        throw failure
    } finally {
        if (cancelled) restore()
    }
}
