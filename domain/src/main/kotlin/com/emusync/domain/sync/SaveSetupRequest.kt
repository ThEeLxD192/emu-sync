package com.emusync.domain.sync

data class SaveSetupRequest(
    val game: SyncGame,
    val cloudFileNames: List<String> = emptyList(),
)
