package com.emusync.config.dto

import kotlinx.serialization.Serializable

@Serializable
internal data class SaveTargetDto(
    val path: String,
    val type: SaveTargetTypeDto,
    /**
     * File name or directory prefix within the game's cloud saves; null keeps the legacy layout.
     */
    val cloudPath: String? = null,
)
