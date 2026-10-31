package com.emusync.drive

internal data class PostSyncResult(
    val completedCount: Int,
    val failedTargets: List<SaveUploadTarget>,
    val blockedReasons: List<String> = emptyList(),
)
