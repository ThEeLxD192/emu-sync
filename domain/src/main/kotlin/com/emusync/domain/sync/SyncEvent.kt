package com.emusync.domain.sync

/** Observable lifecycle events contain data, without UI state or dialog callbacks. */
sealed interface SyncEvent {
    data object Idle : SyncEvent

    data class Syncing(val message: String) : SyncEvent

    data object Playing : SyncEvent

    data class Error(val message: String) : SyncEvent

    data class Conflict(val request: SaveConflict) : SyncEvent

    data class PostSyncFailed(val failure: PostSyncFailure) : SyncEvent
}
