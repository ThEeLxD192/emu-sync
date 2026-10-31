package com.emusync.domain.sync

data class PostSyncFailure(
    val gameName: String,
    val reason: String,
    val completedSaveCount: Int,
    val pendingSaveCount: Int,
)
