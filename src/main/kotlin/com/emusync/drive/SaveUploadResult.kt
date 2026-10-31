package com.emusync.drive

import com.emusync.model.SaveTargetType
import java.io.File

internal data class SaveUploadTarget(
    val file: File,
    val type: SaveTargetType,
    val driveName: String?,
    val failureReason: String? = null,
    val reservedDriveNames: Set<String> = emptySet(),
)

internal data class PostSyncResult(
    val completedCount: Int,
    val failedTargets: List<SaveUploadTarget>,
    val blockedReasons: List<String> = emptyList(),
)
