package com.emusync.application.sync

import com.emusync.domain.library.SaveTargetType
import com.emusync.domain.sync.LocalSavePath

internal data class SaveUploadTarget(
    val file: LocalSavePath,
    val type: SaveTargetType,
    val driveName: String?,
    val failureReason: String? = null,
    val reservedDriveNames: Set<String> = emptySet(),
)
