package com.emusync.ui.library.sync

import com.emusync.ui.library.GameItem

data class SaveSetupRequest(
    val item: GameItem,
    val notice: String? = null,
    val cloudFileNames: List<String> = emptyList(),
)
