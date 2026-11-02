package com.emusync.ui.library

import com.emusync.domain.config.ConfigurationStore
import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.SaveTargetAccess
import com.emusync.domain.library.SteamLibrary
import com.emusync.domain.sync.SaveSynchronizerFactory

/** Borrowed capabilities needed to present and operate the library. */
data class LibraryServices(
    val store: ConfigurationStore,
    val steam: SteamLibrary,
    val synchronizers: SaveSynchronizerFactory,
    val openEmulator: suspend (EmulatorSystem) -> Unit,
    val loadGames: suspend (GameEntry) -> List<GameItem>,
    val saveTargets: SaveTargetAccess,
)
