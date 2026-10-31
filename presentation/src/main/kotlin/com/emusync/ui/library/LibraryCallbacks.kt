package com.emusync.ui.library

import com.emusync.domain.library.GameEntry
import com.emusync.ui.library.sync.GameSaveSetup

internal data class LibraryCallbacks(
    val loadGames: suspend (GameEntry) -> List<GameItem>,
    val refreshConnection: () -> Unit,
    val saveGameSetup: suspend (GameItem, GameSaveSetup) -> Unit,
)
