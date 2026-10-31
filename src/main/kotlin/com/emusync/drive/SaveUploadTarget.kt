package com.emusync.drive

import com.emusync.domain.library.SaveTargetType
import java.io.File

internal data class SaveUploadTarget(
    val file: File,
    val type: SaveTargetType,
    val driveName: String?,
    val failureReason: String? = null,
    val reservedDriveNames: Set<String> = emptySet(),
)
