package com.emusync.application.sync

internal data class PostSyncResult(
    val completedCount: Int,
    val failedTargets: List<SaveUploadTarget>,
    val blockedReasons: List<String> = emptyList(),
)
