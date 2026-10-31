package com.emusync.ui

import com.emusync.model.SaveTarget

internal data class GameOverrideChanges(
    val targets: List<SaveTarget>,
    val coverPath: String?,
    val title: String?,
)
