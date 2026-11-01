package com.emusync.ui.library.form

import androidx.compose.runtime.snapshots.Snapshot
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Resolves one coherent draft off the UI thread while fields may continue to change. */
internal suspend fun <T> readFormDraft(
    ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
    read: () -> T,
): T {
    val snapshot = Snapshot.takeMutableSnapshot()
    return try {
        val result = withContext(ioDispatcher) { snapshot.enter(read) }
        snapshot.apply().check()
        result
    } finally {
        snapshot.dispose()
    }
}
