package com.emusync.ui.library.form

import com.emusync.domain.library.SaveTarget

internal data class GameEditResult(
    val title: String?,
    val targets: List<SaveTarget>,
    val coverPath: String?,
)
