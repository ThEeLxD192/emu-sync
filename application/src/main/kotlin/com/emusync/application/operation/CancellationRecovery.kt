package com.emusync.application.operation

import kotlinx.coroutines.CancellationException

/** Restores transient state before cancellation reaches the caller, retaining the caller's job. */
internal inline fun <T> restoreOnCancellation(action: () -> T, restore: () -> Unit): T {
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
