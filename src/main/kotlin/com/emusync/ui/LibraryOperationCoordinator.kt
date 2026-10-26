package com.emusync.ui

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex
import java.util.concurrent.atomic.AtomicLong

/** Serializes save-changing actions; duplicate clicks are rejected rather than queued. */
internal class LibraryOperationCoordinator(private val state: MutableStateFlow<AppUiState>) {
    private val mutex = Mutex()
    private val generation = AtomicLong()
    val revision: Long get() = generation.get()
    val isBusy: Boolean get() = mutex.isLocked

    suspend fun <T> runIfIdle(block: suspend () -> T): T? {
        if (!mutex.tryLock()) return null
        generation.incrementAndGet()
        state.update { it.copy(isOperationInProgress = true,
            entrySyncStatus = it.entrySyncStatus.mapValues { (_, status) ->
                if (status == CloudSyncStatus.CHECKING) CloudSyncStatus.IDLE else status
            }) }
        try {
            return block()
        } catch (cancelled: CancellationException) {
            state.update { it.copy(status = AppStatus.Idle,
                entrySyncStatus = it.entrySyncStatus.mapValues { (_, status) ->
                    if (status == CloudSyncStatus.SYNCING) CloudSyncStatus.IDLE else status
                }) }
            throw cancelled
        } finally {
            generation.incrementAndGet()
            state.update { it.copy(isOperationInProgress = false) }
            mutex.unlock()
        }
    }
}
