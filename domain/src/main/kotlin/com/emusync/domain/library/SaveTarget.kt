package com.emusync.domain.library

data class SaveTarget(
    val path: String,
    val type: SaveTargetType,
    /**
     * File name or directory prefix within the game's cloud saves; null keeps the legacy layout.
     */
    val cloudPath: String? = null,
)
