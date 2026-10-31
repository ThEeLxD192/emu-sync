package com.emusync.ui

import com.emusync.domain.library.GameEntry

internal data class LibraryCallbacks(
    val loadGames: suspend (GameEntry) -> List<GameItem>,
    val refreshConnection: () -> Unit,
    val saveGameSetup: suspend (GameItem, AppViewModel.SaveSetupResult) -> Unit,
)
