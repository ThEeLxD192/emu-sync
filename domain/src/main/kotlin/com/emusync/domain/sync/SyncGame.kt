package com.emusync.domain.sync

import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.SaveTarget

/** A game selected for synchronization, with its resolved local save destinations. */
data class SyncGame(
    val name: String,
    val entry: GameEntry,
    val romPath: String?,
    val effectiveTitle: String,
    val effectiveSaveTargets: List<SaveTarget>,
)
