package com.emusync.domain.sync

import com.emusync.domain.library.SaveTarget

/** The game reflects any configuration edits made while selecting save destinations. */
data class SaveSetupResult(val game: SyncGame, val targets: List<SaveTarget>)
