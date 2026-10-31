package com.emusync.application.library

import com.emusync.domain.library.SaveTarget

data class GameOverrideChanges(
    val targets: List<SaveTarget>,
    val coverPath: String?,
    val title: String?,
)
