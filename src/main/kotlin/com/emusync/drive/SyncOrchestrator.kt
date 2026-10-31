package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.SaveTarget as ConfiguredSaveTarget
import com.emusync.domain.library.SaveTargetType
import com.emusync.domain.sync.EntrySyncStatus
import com.emusync.domain.sync.PostSyncFailure
import com.emusync.domain.sync.SaveConflict
import com.emusync.domain.sync.SaveSetup
import com.emusync.domain.sync.SaveSynchronizer
import com.emusync.domain.sync.SyncDecision
import com.emusync.domain.sync.SyncEvent
import com.emusync.domain.sync.SyncGame
import com.emusync.model.AppConfig
import io.ktor.client.HttpClient
import java.io.File

internal fun getSiblingReservedNames(
    targetPath: File,
    allTargets: Iterable<Pair<File, SaveTargetType>>,
): Set<String> {
    val reserved = mutableSetOf<String>()
    val targetNormalized = targetPath.normalize().absolutePath

    for ((otherFile, otherType) in allTargets) {
        if (otherFile.normalize().absolutePath == targetNormalized) {
            continue
        }
        val isOtherDir =
            if (otherFile.exists()) {
                otherFile.isDirectory
            } else {
                otherType == SaveTargetType.DIRECTORY
            }

        if (isOtherDir) {
            reserved += newerSiblingPaths(otherFile, targetPath)
        } else {
            reserved.add(otherFile.name)
        }
    }
    return reserved
}

private fun newerSiblingPaths(directory: File, target: File): Set<String> {
    if (!directory.isDirectory) return emptySet()
    return directory
        .walkTopDown()
        .filter { it.isFile }
        .mapNotNull { file ->
            val relative = file.toRelativeString(directory).replace(File.separatorChar, '/')
            val destination = target.resolve(relative)
            relative.takeIf {
                !destination.exists() || file.lastModified() > destination.lastModified()
            }
        }
        .toSet()
}

internal fun isDirectorySaveTarget(
    type: SaveTargetType,
    path: File,
    cloudFiles: List<DriveFile>,
    reservedSiblingNames: Set<String> = emptySet(),
    cloudName: String? = null,
): Boolean {
    if (path.exists()) {
        return path.isDirectory
    }
    return when (type) {
        SaveTargetType.DIRECTORY -> true
        SaveTargetType.FILE -> false
        SaveTargetType.UNKNOWN ->
            cloudFiles.none { cloudFile ->
                cloudFile.name == (cloudName ?: path.name) &&
                    cloudFile.name !in reservedSiblingNames
            }
    }
}

internal fun cloudFilesForSaveTarget(
    type: SaveTargetType,
    path: File,
    cloudFiles: List<DriveFile>,
    reservedSiblingNames: Set<String> = emptySet(),
): List<DriveFile> {
    if (!isDirectorySaveTarget(type, path, cloudFiles, reservedSiblingNames)) return cloudFiles
    return cloudFiles.filterNot { it.name in reservedSiblingNames }
}

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
    client: HttpClient,
    config: AppConfig,
    configManager: ConfigManager,
    onStatus: (SyncEvent) -> Unit,
) : SaveSynchronizer {
    private val context = SyncContext(client, config, configManager, onStatus)
    private val paths = SaveTargetResolver()
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
