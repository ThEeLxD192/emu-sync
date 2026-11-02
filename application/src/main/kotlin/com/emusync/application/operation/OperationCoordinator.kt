package com.emusync.application.operation

import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.sync.Mutex

/** Reject overlapping mutations and invalidate observations before and after each mutation. */
class OperationCoordinator(private val onEvent: (OperationEvent) -> Unit = {}) {
    private val mutex = Mutex()
    private val generation = AtomicLong()
    val revision: Long
        get() = generation.get()

    val isBusy: Boolean
        get() = mutex.isLocked

    suspend fun <T> runIfIdle(block: suspend () -> T): T? {
        if (!mutex.tryLock()) return null
        generation.incrementAndGet()
        try {
            onEvent(OperationEvent.STARTED)
            return restoreOnCancellation(
                action = { block() },
                restore = { onEvent(OperationEvent.CANCELLED) },
            )
        } finally {
            generation.incrementAndGet()
            try {
                onEvent(OperationEvent.FINISHED)
            } finally {
                mutex.unlock()
            }
        }
    }
}
