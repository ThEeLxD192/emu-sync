package com.emusync.application.sync

import com.emusync.domain.library.SaveTarget as ConfiguredSaveTarget
import com.emusync.domain.library.SaveTargetType
import com.emusync.domain.sync.LocalSavePath

internal data class ResolvedSaveTarget(
    val file: LocalSavePath,
    val type: SaveTargetType,
    val cloudPathSegments: List<String>,
    val displayName: String,
    val cloudPath: String? = null,
) {
    fun configured() = ConfiguredSaveTarget(file.path, type, cloudPath)
}
