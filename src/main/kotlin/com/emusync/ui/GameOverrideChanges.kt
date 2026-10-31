package com.emusync.ui

import com.emusync.domain.library.SaveTarget

internal data class GameOverrideChanges(
    val targets: List<SaveTarget>,
    val coverPath: String?,
    val title: String?,
)
