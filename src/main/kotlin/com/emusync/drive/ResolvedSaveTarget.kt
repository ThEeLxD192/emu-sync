package com.emusync.drive

import com.emusync.domain.library.SaveTarget as ConfiguredSaveTarget
import com.emusync.domain.library.SaveTargetType
import java.io.File

internal data class ResolvedSaveTarget(
    val file: File,
    val type: SaveTargetType,
    val cloudPathSegments: List<String>,
    val displayName: String,
    val cloudPath: String? = null,
) {
    fun configured() = ConfiguredSaveTarget(file.path, type, cloudPath)
}
