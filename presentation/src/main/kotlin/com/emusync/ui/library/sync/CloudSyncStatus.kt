package com.emusync.ui.library.sync

/** Cloud sync status for an entry/emulator system (Steam Cloud style). */
enum class CloudSyncStatus {
    IDLE,
    CHECKING,
    IN_SYNC,
    OUT_OF_SYNC,
    SYNCING,
    CONFLICT,
    NOT_CONFIGURED,
    ERROR,
}
