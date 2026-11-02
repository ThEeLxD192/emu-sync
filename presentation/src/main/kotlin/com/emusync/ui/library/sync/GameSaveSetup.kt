package com.emusync.ui.library.sync

import com.emusync.domain.library.SaveTarget

data class GameSaveSetup(
    val targets: List<SaveTarget>,
    val title: String? = null,
    val coverPath: String? = null,
)
