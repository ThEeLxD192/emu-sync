package com.emusync.application.sync

import com.emusync.domain.config.AppConfig
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.SaveTarget as ConfiguredSaveTarget
import com.emusync.domain.sync.EntrySyncStatus
import com.emusync.domain.sync.PostSyncFailure
import com.emusync.domain.sync.SaveConflict
import com.emusync.domain.sync.SaveSetup
import com.emusync.domain.sync.SaveSynchronizer
import com.emusync.domain.sync.SyncDecision
import com.emusync.domain.sync.SyncEvent
import com.emusync.domain.sync.SyncGame

/**
 * Orchestrates the full game lifecycle and standalone save sync:
 *
 * 1. **Check Status**: Detect whether saves are up to date (Steam Cloud style)
 * 2. **Manual Sync**: Sync saves on demand without launching game
 * 3. **Play Lifecycle**: Pre-Sync → Launch Game → Post-Sync
 *
 * Drive folder structure:
 * ```
 * EmuSync/                      ← root folder
 * ├── Game Boy Advance/         ← per GameEntry.name
 * │   └── pokemon_emerald.sav
 * └── Spelunky Classic/
 *     └── save.dat
 * ```
 */
class SyncOrchestrator(
    config: AppConfig,
    services: SyncServices,
    onStatus: (SyncEvent) -> Unit,
) : SaveSynchronizer {
    private val context = SyncContext(config, services, onStatus)
    private val paths = SaveTargetResolver(services.files)
    private val manual = ManualFileSync(context)
    private val status = SyncStatusChecker(context, paths)
    private val entrySync = ManualEntrySync(context, paths, manual)
    private val migration = CloudEntryMigration(context, paths, manual)
    private val lifecycle = GameSyncLifecycle(context, paths, manual)

    override suspend fun checkEntrySyncStatus(entry: GameEntry): EntrySyncStatus =
        status.checkEntrySyncStatus(entry)

    override suspend fun syncEntry(entry: GameEntry): Boolean = entrySync.syncEntry(entry)

    override suspend fun renameGameCloudData(
        entry: GameEntry,
        romName: String?,
        oldTitle: String,
        newTitle: String,
        localSaveTargets: List<ConfiguredSaveTarget>,
    ): Boolean = migration.renameGameCloudData(entry, romName, oldTitle, newTitle, localSaveTargets)

    override suspend fun renameEntryCloudData(oldEntry: GameEntry, newEntry: GameEntry): Boolean =
        migration.renameEntryCloudData(oldEntry, newEntry)

    override suspend fun playWithSync(game: SyncGame, setup: SaveSetup) =
        lifecycle.playWithSync(game, setup)

    override fun resolveConflict(request: SaveConflict, decision: SyncDecision) =
        context.resolveConflict(request, decision)

    override suspend fun retryPostSync(failure: PostSyncFailure) = context.retryPostSync(failure)
}
