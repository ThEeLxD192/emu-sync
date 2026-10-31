package com.emusync.domain.sync

import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.SaveTarget

/** Entry and session synchronization independently of the cloud provider and presentation. */
interface SaveSynchronizer {
    suspend fun checkEntrySyncStatus(entry: GameEntry): EntrySyncStatus

    suspend fun syncEntry(entry: GameEntry): Boolean

    suspend fun renameGameCloudData(
        entry: GameEntry,
        romName: String?,
        oldTitle: String,
        newTitle: String,
        localSaveTargets: List<SaveTarget>,
    ): Boolean

    suspend fun renameEntryCloudData(oldEntry: GameEntry, newEntry: GameEntry): Boolean

    suspend fun playWithSync(
        game: SyncGame,
        setup: SaveSetup = SaveSetup {
            SaveSetupResult(it.game, emptyList())
        },
    )

    /** Replies to the exact request from an event; cancelled or completed requests are ignored. */
    fun resolveConflict(request: SaveConflict, decision: SyncDecision)

    /** Retries the reported failure once; an older failure cannot replay a newer batch. */
    suspend fun retryPostSync(failure: PostSyncFailure)
}
