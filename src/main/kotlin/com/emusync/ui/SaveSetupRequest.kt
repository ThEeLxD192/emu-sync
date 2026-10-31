package com.emusync.ui

data class SaveSetupRequest(
    val item: GameItem,
    val notice: String? = null,
    val cloudFileNames: List<String> = emptyList(),
)
