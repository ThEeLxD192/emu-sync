package com.emusync.ui

import com.emusync.domain.sync.SyncDecision

/** Current status of the game launch/sync lifecycle. */
sealed interface AppStatus {
    data object Idle : AppStatus

    data object Loading : AppStatus

    data class Syncing(val message: String) : AppStatus

    data object Playing : AppStatus

    data class Error(val message: String) : AppStatus

    data class PostSyncFailed(
        val gameName: String,
        val reason: String,
        val completedSaveCount: Int,
        val pendingSaveCount: Int,
        val onRetry: suspend () -> Unit,
    ) : AppStatus

    data class Conflict(
        val localDate: String,
        val cloudDate: String,
        val gameName: String,
        val onResolve: (SyncDecision) -> Unit,
    ) : AppStatus
}
