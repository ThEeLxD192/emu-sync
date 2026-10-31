package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.model.AppConfig
import com.emusync.model.GameEntry
import com.emusync.model.SaveTarget as ConfiguredSaveTarget
import com.emusync.model.SaveTargetType
import com.emusync.ui.AppStatus
import com.emusync.ui.CloudSyncStatus
import com.emusync.ui.GameItem
import io.ktor.client.HttpClient
import java.io.File
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter

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
    onStatus: (AppStatus) -> Unit,
) {
    private val context = SyncContext(client, config, configManager, onStatus)
    private val paths = SaveTargetResolver()
    private val manual = ManualFileSync(context)
    private val status = SyncStatusChecker(context, paths)
    private val entrySync = ManualEntrySync(context, paths, manual)
    private val migration = CloudEntryMigration(context, paths, manual)
    private val lifecycle = GameSyncLifecycle(context, paths, manual)

    data class SaveTarget(
        val file: File,
        val type: SaveTargetType,
        val cloudPathSegments: List<String>,
        val displayName: String,
        val cloudPath: String? = null,
    ) {
        fun configured() = ConfiguredSaveTarget(file.path, type, cloudPath)
    }

    fun sanitizeCloudFolderName(name: String): String = paths.sanitizeCloudFolderName(name)

    fun getCloudPathSegments(
        entry: GameEntry,
        romName: String? = null,
        gameTitle: String? = null,
    ): List<String> = paths.getCloudPathSegments(entry, romName, gameTitle)

    fun getCloudPathSegments(item: GameItem): List<String> = paths.getCloudPathSegments(item)

    fun getSaveTargets(entry: GameEntry): List<SaveTarget> = paths.getSaveTargets(entry)

    fun getAllSavePaths(entry: GameEntry): List<File> = paths.getAllSavePaths(entry)

    suspend fun checkEntrySyncStatus(entry: GameEntry): CloudSyncStatus =
        status.checkEntrySyncStatus(entry)

    suspend fun syncEntry(entry: GameEntry): Boolean = entrySync.syncEntry(entry)

    suspend fun renameGameCloudData(
        entry: GameEntry,
        romName: String?,
        oldTitle: String,
        newTitle: String,
        localSaveTargets: List<ConfiguredSaveTarget>,
    ): Boolean = migration.renameGameCloudData(entry, romName, oldTitle, newTitle, localSaveTargets)

    suspend fun renameEntryCloudData(oldEntry: GameEntry, newEntry: GameEntry): Boolean =
        migration.renameEntryCloudData(oldEntry, newEntry)

    suspend fun playWithSync(
        item: GameItem,
        resolveConfiguredItem: (GameItem) -> GameItem = { it },
        onRequestSaveSetup:
            suspend (item: GameItem, notice: String?, cloudFileNames: List<String>) -> List<
                    ConfiguredSaveTarget
                > =
            { _, _, _ ->
                emptyList()
            },
    ) = lifecycle.playWithSync(item, resolveConfiguredItem, onRequestSaveSetup)

    companion object {
        private val dateFormatter =
            DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss").withZone(ZoneId.systemDefault())

        fun formatTimestamp(epochMs: Long): String =
            dateFormatter.format(Instant.ofEpochMilli(epochMs))
    }
}
