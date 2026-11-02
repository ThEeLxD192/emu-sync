package com.emusync.domain.sync

enum class EntrySyncStatus {
    IDLE,
    IN_SYNC,
    OUT_OF_SYNC,
    CONFLICT,
    NOT_CONFIGURED,
    ERROR,
}
