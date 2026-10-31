package com.emusync.domain.sync

/** Timestamps stay numeric until the presentation boundary formats them. */
data class SaveConflict(
    val localModifiedTimeMs: Long,
    val cloudModifiedTimeMs: Long,
    val gameName: String,
)
