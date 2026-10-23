package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.model.AppConfig
import com.emusync.model.GameEntry
import com.emusync.model.effectiveCloudFolder
import com.emusync.model.EmulatorSystem
import com.emusync.model.NativePCGame
import com.emusync.model.SaveTargetType
import com.emusync.model.SaveTarget as ConfiguredSaveTarget
import com.emusync.model.effectiveSaveTargets
import com.emusync.model.effectiveSaveTargetsForRom
import com.emusync.model.inferLegacySaveTarget
import com.emusync.runner.GameRunner
import com.emusync.ui.AppStatus
import com.emusync.ui.CloudSyncStatus
import com.emusync.ui.GameItem
import io.ktor.client.HttpClient
import kotlinx.coroutines.CancellationException
import java.io.File
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.logging.Level
import java.util.logging.Logger

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
        val isOtherDir = if (otherFile.exists()) {
            otherFile.isDirectory
        } else {
            otherType == SaveTargetType.DIRECTORY
        }

        if (isOtherDir) {
            if (otherFile.exists() && otherFile.isDirectory) {
                val files = otherFile.walkTopDown().filter { it.isFile }
                for (file in files) {
                    val relPath = file.toRelativeString(otherFile).replace(File.separatorChar, '/')
                    val fileInTarget = targetPath.resolve(relPath)
                    if (!fileInTarget.exists() || file.lastModified() > fileInTarget.lastModified()) {
                        reserved.add(relPath)
                    }
                }
            }
        } else {
            reserved.add(otherFile.name)
        }
    }
    return reserved
}

internal fun isDirectorySaveTarget(
    type: SaveTargetType,
    path: File,
    cloudFiles: List<DriveFile>,
    reservedSiblingNames: Set<String> = emptySet(),
): Boolean {
    if (path.exists()) {
        return path.isDirectory
    }
    return when (type) {
        SaveTargetType.DIRECTORY -> true
        SaveTargetType.FILE -> false
        SaveTargetType.UNKNOWN -> cloudFiles.none { cloudFile ->
            cloudFile.name == path.name && cloudFile.name !in reservedSiblingNames
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
    private val client: HttpClient,
    private val config: AppConfig,
    private val configManager: ConfigManager,
    private val onStatus: (AppStatus) -> Unit,
) {
    private data class SaveUploadTarget(
        val file: File,
        val type: SaveTargetType,
        val driveName: String?,
        val failureReason: String? = null,
        val reservedDriveNames: Set<String> = emptySet(),
    )

    private data class PostSyncResult(
        val completedCount: Int,
        val failedTargets: List<SaveUploadTarget>,
        val blockedReasons: List<String> = emptyList(),
    )

    private val logger = Logger.getLogger(SyncOrchestrator::class.java.name)
    private val search = DriveSearch(client)
    private val transfer = DriveTransfer(client) { progress ->
        val verb = if (progress.direction == TransferProgress.Direction.DOWNLOAD) "Downloading" else "Uploading"
        val percent = progress.totalBytes
            ?.takeIf { it > 0L }
            ?.let { ((progress.transferredBytes * 100L) / it).coerceIn(0L, 100L) }
        val suffix = percent?.let { " ($it%)" }.orEmpty()
        onStatus(AppStatus.Syncing("$verb ${progress.fileName}$suffix"))
    }
    private val folders = DriveFolders(client)
    private val oauthFlow = OAuthFlow(client)
    private val runner = GameRunner()

    data class SaveTarget(
        val file: File,
        val type: SaveTargetType,
        val cloudPathSegments: List<String>,
        val displayName: String,
    )

    fun sanitizeCloudFolderName(name: String): String {
        return name
            .replace('/', '-')
            .replace('\\', '-')
            .trim()
            .ifBlank { "Unknown" }
    }

    fun getCloudPathSegments(entry: GameEntry, romName: String? = null, gameTitle: String? = null): List<String> {
        val baseFolder = entry.effectiveCloudFolder
        return when (entry) {
            is EmulatorSystem -> {
                val globalPaths = entry.effectiveSaveTargets()
                    .map { normalizeSavePath(it.path) }
                    .toSet()
                val romPaths = romName?.let { name ->
                    entry.effectiveSaveTargetsForRom(name).map { it.path }
                }.orEmpty()
                val hasDistinctRomPath = romPaths.any { normalizeSavePath(it) !in globalPaths }

                // A configured global save path uses the emulator folder unless this ROM has
                // a real, distinct path of its own. Empty or copied global-path entries are
                // legacy per-ROM override records, not per-game save configuration.
                if (romName != null && (hasDistinctRomPath || globalPaths.isEmpty())) {
                    val title = gameTitle ?: entry.getEffectiveTitle(romName, romName)
                    listOf(baseFolder, sanitizeCloudFolderName(title))
                } else {
                    listOf(baseFolder)
                }
            }
            is NativePCGame -> {
                if (entry.group != null && entry.cloudFolder == null) {
                    listOf(baseFolder, sanitizeCloudFolderName(entry.name))
                } else {
                    listOf(baseFolder)
                }
            }
        }
    }

    private fun normalizeSavePath(path: String): String =
        path.trim().replace('\\', '/').trimEnd('/').ifBlank { "/" }

    fun getCloudPathSegments(item: GameItem): List<String> {
        val romName = item.romFile?.name ?: item.name
        return getCloudPathSegments(item.entry, romName, item.effectiveTitle)
    }

    fun getSaveTargets(entry: GameEntry): List<SaveTarget> {
        return when (entry) {
            is NativePCGame -> {
                val cloudPath = getCloudPathSegments(entry)
                entry.effectiveSaveTargets().map {
                    SaveTarget(File(it.path), it.type, cloudPath, entry.name)
                }
            }
            is EmulatorSystem -> {
                val targets = mutableListOf<SaveTarget>()

                // 1. Global emulator save paths (if any)
                if (entry.effectiveSaveTargets().isNotEmpty()) {
                    val globalPath = listOf(entry.effectiveCloudFolder)
                    for (target in entry.effectiveSaveTargets()) {
                        targets.add(SaveTarget(File(target.path), target.type, globalPath, entry.name))
                    }
                }

                // 2. Per-ROM save paths
                val romNames = entry.saveTargetsByRom.keys + entry.savePathsByRom.keys
                for (romName in romNames) {
                    val title = entry.getEffectiveTitle(romName, romName)
                    val romCloudPath = listOf(entry.effectiveCloudFolder, sanitizeCloudFolderName(title))
                    for (target in entry.effectiveSaveTargetsForRom(romName)) {
                        targets.add(SaveTarget(File(target.path), target.type, romCloudPath, title))
                    }
                }

                targets.distinctBy { it.file.absolutePath }
            }
        }
    }

    /**
     * Helper to retrieve all configured save paths for an entry.
     */
    fun getAllSavePaths(entry: GameEntry): List<File> {
        return getSaveTargets(entry).map { it.file }
    }

    /**
     * Checks whether an entry's save files are up to date with Google Drive.
     * Returns [CloudSyncStatus] without launching any emulator or altering files.
     */
    suspend fun checkEntrySyncStatus(entry: GameEntry): CloudSyncStatus {
        val driveConfig = config.googleDrive
        if (driveConfig == null || driveConfig.refreshToken.isNullOrBlank()) {
            return CloudSyncStatus.NOT_CONFIGURED
        }

        val targets = getSaveTargets(entry)
        if (targets.isEmpty()) {
            return CloudSyncStatus.IDLE
        }

        return try {
            val token = oauthFlow.authorize(config, configManager, allowInteractive = false)

            var hasNewerLocal = false
            var hasNewerCloud = false
            var hasConflict = false
            var anyFileChecked = false

            val targetsByFolder = targets.groupBy { it.cloudPathSegments }

            for ((cloudPath, folderTargets) in targetsByFolder) {
                val folderId = try {
                    folders.findPath(token, cloudPath)
                } catch (_: Exception) {
                    null
                }
                val cloudFiles = if (folderId != null) {
                    search.listFilesInFolder(token, folderId)
                } else {
                    emptyList()
                }

                val targetPairs = folderTargets.map { it.file to it.type }

                for (target in folderTargets) {
                    val savePath = target.file
                    val reservedSiblingNames = getSiblingReservedNames(savePath, targetPairs)
                    val isDirectory = isDirectorySaveTarget(
                        target.type, savePath, cloudFiles, reservedSiblingNames,
                    )

                    if (isDirectory) {
                        val targetCloudFiles = cloudFilesForSaveTarget(
                            target.type, savePath, cloudFiles, reservedSiblingNames,
                        )
                        val localFiles = if (savePath.exists() && savePath.isDirectory) {
                            savePath.walkTopDown().filter { it.isFile }.toList()
                        } else {
                            emptyList()
                        }

                        if (localFiles.none { it.toRelativeString(savePath).replace(File.separatorChar, '/') !in reservedSiblingNames } && targetCloudFiles.isEmpty()) continue
                        anyFileChecked = true

                        for (cloudFile in targetCloudFiles) {
                            val localFile = resolveRemoteFile(savePath, cloudFile.name) ?: continue
                            val decision = planSyncDecision(
                                localModifiedTimeMs = localFile.takeIf { it.exists() }?.lastModified(),
                                cloudModifiedTimeIso = cloudFile.modifiedTime,
                            )
                            when (decision) {
                                SyncDecision.DOWNLOAD_CLOUD -> hasNewerCloud = true
                                SyncDecision.CONFLICT -> hasConflict = true
                                SyncDecision.UPLOAD_LOCAL -> hasNewerLocal = true
                                SyncDecision.IN_SYNC -> {}
                            }
                        }

                        for (localFile in localFiles) {
                            val relPath = localFile.toRelativeString(savePath).replace(File.separatorChar, '/')
                            if (relPath in reservedSiblingNames) continue
                            if (targetCloudFiles.none { it.name == relPath }) {
                                if (planSyncDecision(localFile.lastModified(), null) == SyncDecision.UPLOAD_LOCAL) {
                                    hasNewerLocal = true
                                }
                            }
                        }
                    } else {
                        val cloudFile = cloudFiles.find { it.name == savePath.name }
                        val localExists = savePath.exists()
                        val cloudExists = cloudFile != null

                        val decision = planSyncDecision(
                            localModifiedTimeMs = savePath.takeIf { localExists }?.lastModified(),
                            cloudModifiedTimeIso = cloudFile?.modifiedTime,
                        )
                        if (decision == SyncDecision.IN_SYNC && !localExists && !cloudExists) continue
                        anyFileChecked = true

                        when (decision) {
                            SyncDecision.DOWNLOAD_CLOUD -> hasNewerCloud = true
                            SyncDecision.CONFLICT -> hasConflict = true
                            SyncDecision.UPLOAD_LOCAL -> hasNewerLocal = true
                            SyncDecision.IN_SYNC -> {}
                        }
                    }
                }
            }

            when {
                !anyFileChecked -> CloudSyncStatus.IN_SYNC
                hasConflict || (hasNewerLocal && hasNewerCloud) -> CloudSyncStatus.CONFLICT
                hasNewerLocal || hasNewerCloud -> CloudSyncStatus.OUT_OF_SYNC
                else -> CloudSyncStatus.IN_SYNC
            }
        } catch (e: Exception) {
            CloudSyncStatus.ERROR
        }
    }

    /**
     * Manually synchronizes all saves for a given entry with Google Drive
     * WITHOUT launching the game or emulator.
     */
    suspend fun syncEntry(entry: GameEntry): Boolean {
        return try {
            val driveConfig = config.googleDrive
            if (driveConfig == null || driveConfig.refreshToken.isNullOrBlank()) {
                onStatus(AppStatus.Error("Google Drive is not connected. Click 'Connect Drive' in the top bar to sign in."))
                return false
            }

            onStatus(AppStatus.Syncing("Connecting to Google Drive..."))
            val token = try {
                oauthFlow.authorize(config, configManager, allowInteractive = false)
            } catch (e: Exception) {
                onStatus(AppStatus.Error("Could not connect to Google Drive (${e.message ?: "session expired"}). Please reconnect your account by clicking 'Connect Drive'."))
                return false
            }

            val targets = getSaveTargets(entry)
            if (targets.isEmpty()) {
                onStatus(AppStatus.Idle)
                return true
            }

            val targetsByFolder = targets.groupBy { it.cloudPathSegments }

            for ((cloudPath, folderTargets) in targetsByFolder) {
                val folderId = folders.ensurePath(token, cloudPath)
                val cloudFiles = search.listFilesInFolder(token, folderId)
                val targetPairs = folderTargets.map { it.file to it.type }

                for (target in folderTargets) {
                    val savePath = target.file
                    onStatus(AppStatus.Syncing("Syncing ${target.displayName}..."))
                    val reservedSiblingNames = getSiblingReservedNames(savePath, targetPairs)
                    val isDirectory = isDirectorySaveTarget(
                        target.type, savePath, cloudFiles, reservedSiblingNames,
                    )

                    if (isDirectory) {
                        val targetCloudFiles = cloudFilesForSaveTarget(
                            target.type, savePath, cloudFiles, reservedSiblingNames,
                        )
                        syncDirectoryManual(
                            token, entry, savePath, folderId, targetCloudFiles,
                            displayName = target.displayName,
                            reservedSiblingNames = reservedSiblingNames,
                        )
                    } else {
                        syncFileManual(token, entry, savePath, folderId, cloudFiles, displayName = target.displayName)
                    }
                }
            }

            onStatus(AppStatus.Idle)
            true
        } catch (e: Exception) {
            onStatus(AppStatus.Error("Sync error: ${e.message}"))
            false
        }
    }

    /**
     * Handles renaming a game's cloud folder on Google Drive when the user changes its title.
     *
     * 1. If old folder exists and new folder does not: renames/moves the old folder to the new name.
     * 2. If new folder already exists (e.g. from another device):
     *    - Moves non-conflicting files from old folder to new folder.
     *    - Deletes old folder.
     *    - Performs conflict check between local save files and cloud saves in new folder.
     *    - Prompts user with [AppStatus.Conflict] if needed so they decide local vs cloud.
     */
    suspend fun renameGameCloudData(
        entry: GameEntry,
        romName: String?,
        oldTitle: String,
        newTitle: String,
        localSaveTargets: List<ConfiguredSaveTarget>,
    ): Boolean {
        val oldCloudPath = getCloudPathSegments(entry, romName, oldTitle)
        val newCloudPath = getCloudPathSegments(entry, romName, newTitle)

        if (oldCloudPath == newCloudPath) return true

        val driveConfig = config.googleDrive ?: return true
        if (driveConfig.refreshToken.isNullOrBlank()) return true

        return try {
            onStatus(AppStatus.Syncing("Checking cloud folder for $newTitle..."))
            val token = try {
                oauthFlow.authorize(config, configManager, allowInteractive = false)
            } catch (e: Exception) {
                onStatus(AppStatus.Error("Drive authentication failed: ${e.message}"))
                return false
            }

            val oldFolderId = folders.findPath(token, oldCloudPath)
            val newFolderId = folders.findPath(token, newCloudPath)

            when {
                // Case 1: Old folder exists, but new folder does NOT exist yet -> Rename old folder!
                oldFolderId != null && newFolderId == null -> {
                    onStatus(AppStatus.Syncing("Renaming cloud folder to $newTitle..."))
                    val oldParentSegments = oldCloudPath.dropLast(1)
                    val newParentSegments = newCloudPath.dropLast(1)

                    if (oldParentSegments == newParentSegments) {
                        folders.renameFolder(token, oldFolderId, newCloudPath.last())
                    } else {
                        val newParentId = folders.ensurePath(token, newParentSegments)
                        val oldParentId = folders.findPath(token, oldParentSegments)
                        folders.renameFolder(
                            accessToken = token,
                            folderId = oldFolderId,
                            newName = newCloudPath.last(),
                            addParentId = newParentId,
                            removeParentId = oldParentId,
                        )
                    }
                    onStatus(AppStatus.Idle)
                    true
                }

                // Case 2: New folder ALREADY exists! (with or without old folder) -> Check conflict and let user decide!
                newFolderId != null -> {
                    onStatus(AppStatus.Syncing("Checking cloud files in $newTitle..."))

                    // If old folder also existed, move non-conflicting files over and delete old folder
                    if (oldFolderId != null && oldFolderId != newFolderId) {
                        val oldFiles = search.listFilesInFolder(token, oldFolderId)
                        val newFiles = search.listFilesInFolder(token, newFolderId)

                        for (file in oldFiles) {
                            if (newFiles.none { it.name == file.name }) {
                                try {
                                    folders.moveFile(token, file.id, addParentId = newFolderId, removeParentId = oldFolderId)
                                } catch (_: Exception) {}
                            }
                        }
                        try {
                            folders.deleteFolder(token, oldFolderId)
                        } catch (_: Exception) {}
                    }

                    // Now check conflict between local save paths and newFolderId
                    val cloudFiles = search.listFilesInFolder(token, newFolderId)
                    val targetPairs = localSaveTargets.map { File(it.path) to it.type }

                    var hasConflict = false
                    var hasNewerLocal = false
                    var hasNewerCloud = false
                    val relevantCloudFiles = mutableListOf<DriveFile>()
                    val relevantLocalFiles = mutableListOf<File>()

                    for (target in localSaveTargets) {
                        val savePath = File(target.path)
                        val reservedSiblingNames = getSiblingReservedNames(savePath, targetPairs)
                        val isDirectory = isDirectorySaveTarget(
                            target.type, savePath, cloudFiles, reservedSiblingNames,
                        )

                        if (isDirectory) {
                            val targetCloudFiles = cloudFilesForSaveTarget(
                                target.type, savePath, cloudFiles, reservedSiblingNames,
                            )
                            val localDirFiles = if (savePath.exists() && savePath.isDirectory) {
                                savePath.walkTopDown().filter {
                                    it.isFile && it.toRelativeString(savePath).replace(File.separatorChar, '/') !in reservedSiblingNames
                                }.toList()
                            } else emptyList()

                            for (cloudFile in targetCloudFiles) {
                                val localF = resolveRemoteFile(savePath, cloudFile.name) ?: continue
                                if (!localF.exists()) {
                                    hasNewerCloud = true
                                    relevantCloudFiles += cloudFile
                                } else {
                                    val decision = resolveConflict(localF.lastModified(), cloudFile.modifiedTime)
                                    when (decision) {
                                        SyncDecision.CONFLICT -> {
                                            hasConflict = true
                                            relevantCloudFiles += cloudFile
                                            relevantLocalFiles += localF
                                        }
                                        SyncDecision.DOWNLOAD_CLOUD -> {
                                            hasNewerCloud = true
                                            relevantCloudFiles += cloudFile
                                            relevantLocalFiles += localF
                                        }
                                        SyncDecision.UPLOAD_LOCAL -> {
                                            hasNewerLocal = true
                                            relevantLocalFiles += localF
                                        }
                                        SyncDecision.IN_SYNC -> {}
                                    }
                                }
                            }
                            for (localF in localDirFiles) {
                                val rel = localF.toRelativeString(savePath).replace(File.separatorChar, '/')
                                if (rel in reservedSiblingNames) continue
                                if (targetCloudFiles.none { it.name == rel }) {
                                    hasNewerLocal = true
                                    relevantLocalFiles += localF
                                }
                            }
                        } else {
                            val cloudFile = cloudFiles.find { it.name == savePath.name }
                            val localExists = savePath.exists()
                            val cloudExists = cloudFile != null

                            if (!localExists && cloudFile != null) {
                                hasNewerCloud = true
                                relevantCloudFiles += cloudFile
                            } else if (localExists && !cloudExists) {
                                hasNewerLocal = true
                                relevantLocalFiles += savePath
                            } else if (localExists && cloudFile != null) {
                                val decision = resolveConflict(savePath.lastModified(), cloudFile.modifiedTime)
                                when (decision) {
                                    SyncDecision.CONFLICT -> {
                                        hasConflict = true
                                        relevantCloudFiles += cloudFile
                                        relevantLocalFiles += savePath
                                    }
                                    SyncDecision.DOWNLOAD_CLOUD -> {
                                        hasNewerCloud = true
                                        relevantCloudFiles += cloudFile
                                        relevantLocalFiles += savePath
                                    }
                                    SyncDecision.UPLOAD_LOCAL -> {
                                        hasNewerLocal = true
                                        relevantLocalFiles += savePath
                                    }
                                    SyncDecision.IN_SYNC -> {}
                                }
                            }
                        }
                    }

                    if (hasConflict || (hasNewerLocal && hasNewerCloud)) {
                        val latestLocalMs = relevantLocalFiles.maxOfOrNull { it.lastModified() }
                            ?: System.currentTimeMillis()
                        val latestCloudMs = relevantCloudFiles.maxOfOrNull {
                            Instant.parse(it.modifiedTime).toEpochMilli()
                        } ?: System.currentTimeMillis()

                        val choice: SyncDecision = kotlinx.coroutines.suspendCancellableCoroutine { continuation ->
                            onStatus(AppStatus.Conflict(
                                localDate = formatTimestamp(latestLocalMs),
                                cloudDate = formatTimestamp(latestCloudMs),
                                gameName = newTitle,
                                onResolve = { userChoice ->
                                    if (continuation.isActive) continuation.resumeWith(Result.success(userChoice))
                                }
                            ))
                        }

                        if (choice == SyncDecision.DOWNLOAD_CLOUD) {
                            onStatus(AppStatus.Syncing("Downloading cloud saves for $newTitle..."))
                            for (target in localSaveTargets) {
                                val savePath = File(target.path)
                                val reservedSiblingNames = getSiblingReservedNames(savePath, targetPairs)
                                val isDirectory = isDirectorySaveTarget(
                                    target.type, savePath, cloudFiles, reservedSiblingNames,
                                )
                                if (isDirectory) {
                                    val targetCloudFiles = cloudFilesForSaveTarget(
                                        target.type, savePath, cloudFiles, reservedSiblingNames,
                                    )
                                    for (cloudFile in targetCloudFiles) {
                                        val localFile = resolveRemoteFile(savePath, cloudFile.name) ?: continue
                                        localFile.parentFile?.mkdirs()
                                        transfer.download(token, cloudFile.id, localFile, cloudFile.modifiedTime)
                                    }
                                } else {
                                    val cloudFile = cloudFiles.find { it.name == savePath.name }
                                    if (cloudFile != null) {
                                        savePath.parentFile?.mkdirs()
                                        transfer.download(token, cloudFile.id, savePath, cloudFile.modifiedTime)
                                    }
                                }
                            }
                        } else if (choice == SyncDecision.UPLOAD_LOCAL) {
                            onStatus(AppStatus.Syncing("Uploading local saves for $newTitle..."))
                            for (target in localSaveTargets) {
                                val savePath = File(target.path)
                                val reservedSiblingNames = getSiblingReservedNames(savePath, targetPairs)
                                if (isDirectorySaveTarget(
                                        target.type, savePath, cloudFiles, reservedSiblingNames,
                                    )) {
                                    val targetCloudFiles = cloudFilesForSaveTarget(
                                        target.type, savePath, cloudFiles, reservedSiblingNames,
                                    )
                                    val localFiles = savePath.walkTopDown().filter {
                                        it.isFile && it.toRelativeString(savePath).replace(File.separatorChar, '/') !in reservedSiblingNames
                                    }.toList()
                                    for (localFile in localFiles) {
                                        val relPath = localFile.toRelativeString(savePath).replace(File.separatorChar, '/')
                                        val cloudFile = targetCloudFiles.find { it.name == relPath }
                                        syncSingleFilePost(token, localFile, cloudFile, newFolderId, relPath, force = true)
                                    }
                                } else if (savePath.exists()) {
                                    val cloudFile = cloudFiles.find { it.name == savePath.name }
                                    syncSingleFilePost(token, savePath, cloudFile, newFolderId, savePath.name, force = true)
                                }
                            }
                        }
                    } else if (hasNewerCloud && !hasNewerLocal) {
                        onStatus(AppStatus.Syncing("Downloading cloud saves for $newTitle..."))
                        for (target in localSaveTargets) {
                            val savePath = File(target.path)
                            val reservedSiblingNames = getSiblingReservedNames(savePath, targetPairs)
                            val isDirectory = isDirectorySaveTarget(
                                target.type, savePath, cloudFiles, reservedSiblingNames,
                            )
                            if (isDirectory) {
                                val targetCloudFiles = cloudFilesForSaveTarget(
                                    target.type, savePath, cloudFiles, reservedSiblingNames,
                                )
                                for (cloudFile in targetCloudFiles) {
                                    val localFile = resolveRemoteFile(savePath, cloudFile.name) ?: continue
                                    localFile.parentFile?.mkdirs()
                                    transfer.download(token, cloudFile.id, localFile, cloudFile.modifiedTime)
                                }
                            } else {
                                val cloudFile = cloudFiles.find { it.name == savePath.name }
                                if (cloudFile != null) {
                                    savePath.parentFile?.mkdirs()
                                    transfer.download(token, cloudFile.id, savePath, cloudFile.modifiedTime)
                                }
                            }
                        }
                    } else if (hasNewerLocal && !hasNewerCloud) {
                        onStatus(AppStatus.Syncing("Uploading local saves for $newTitle..."))
                        for (target in localSaveTargets) {
                            val savePath = File(target.path)
                            val reservedSiblingNames = getSiblingReservedNames(savePath, targetPairs)
                            if (isDirectorySaveTarget(
                                    target.type, savePath, cloudFiles, reservedSiblingNames,
                                )) {
                                val targetCloudFiles = cloudFilesForSaveTarget(
                                    target.type, savePath, cloudFiles, reservedSiblingNames,
                                )
                                val localFiles = savePath.walkTopDown().filter {
                                    it.isFile && it.toRelativeString(savePath).replace(File.separatorChar, '/') !in reservedSiblingNames
                                }.toList()
                                for (localFile in localFiles) {
                                    val relPath = localFile.toRelativeString(savePath).replace(File.separatorChar, '/')
                                    val cloudFile = targetCloudFiles.find { it.name == relPath }
                                    syncSingleFilePost(token, localFile, cloudFile, newFolderId, relPath, force = false)
                                }
                            } else if (savePath.exists()) {
                                val cloudFile = cloudFiles.find { it.name == savePath.name }
                                syncSingleFilePost(token, savePath, cloudFile, newFolderId, savePath.name, force = false)
                            }
                        }
                    }

                    onStatus(AppStatus.Idle)
                    true
                }

                // Case 3: Neither old nor new folder exists in Drive
                else -> {
                    onStatus(AppStatus.Idle)
                    true
                }
            }
        } catch (e: Exception) {
            onStatus(AppStatus.Error("Cloud rename error: ${e.message}"))
            false
        }
    }

    suspend fun renameEntryCloudFolder(
        oldCloudFolder: String,
        newCloudFolder: String,
    ): Boolean {
        if (oldCloudFolder == newCloudFolder) return true
        val driveConfig = config.googleDrive ?: return true
        if (driveConfig.refreshToken.isNullOrBlank()) return true

        return try {
            val token = oauthFlow.authorize(config, configManager, allowInteractive = false)
            val oldFolderId = folders.findPath(token, listOf(oldCloudFolder))
            val newFolderId = folders.findPath(token, listOf(newCloudFolder))

            if (oldFolderId != null && newFolderId == null) {
                folders.renameFolder(token, oldFolderId, newCloudFolder)
            } else if (oldFolderId != null && newFolderId != null) {
                val subFiles = search.listFilesInFolder(token, oldFolderId)
                for (file in subFiles) {
                    try {
                        folders.moveFile(token, file.id, addParentId = newFolderId, removeParentId = oldFolderId)
                    } catch (_: Exception) {}
                }
                try {
                    folders.deleteFolder(token, oldFolderId)
                } catch (_: Exception) {}
            }
            true
        } catch (_: Exception) {
            false
        }
    }

    private suspend fun syncDirectoryManual(
        token: String,
        entry: GameEntry,
        localSavePath: File,
        folderId: String,
        cloudFiles: List<DriveFile>,
        displayName: String = entry.name,
        reservedSiblingNames: Set<String> = emptySet(),
    ) {
        val localFiles = if (localSavePath.exists() && localSavePath.isDirectory) {
            localSavePath.walkTopDown().filter {
                it.isFile && it.toRelativeString(localSavePath).replace(File.separatorChar, '/') !in reservedSiblingNames
            }.toList()
        } else {
            emptyList()
        }

        data class FilePair(val localFile: File, val cloudFile: DriveFile, val decision: SyncDecision)
        val validCloudFiles = cloudFiles.filter { resolveRemoteFile(localSavePath, it.name) != null }
        val actions = planDirectorySync(
            localFiles = localFiles.map {
                LocalFileState(it.toRelativeString(localSavePath).replace(File.separatorChar, '/'), it.lastModified())
            },
            cloudFiles = validCloudFiles.map { CloudFileState(it.id, it.name, it.modifiedTime) },
        )
        val cloudById = validCloudFiles.associateBy { it.id }
        val pairs = actions.mapNotNull { action ->
            val cloud = action.cloud?.let { cloudById[it.id] } ?: return@mapNotNull null
            val localFile = resolveRemoteFile(localSavePath, action.relativePath) ?: return@mapNotNull null
            FilePair(localFile, cloud, action.decision)
        }
        val plannedCloudByPath = pairs.associate { pair ->
            pair.localFile.toRelativeString(localSavePath).replace(File.separatorChar, '/') to pair.cloudFile
        }

        val conflictingPairs = pairs.filter { it.decision == SyncDecision.CONFLICT }
        val hasLocalOnly = actions.any { it.cloud == null && it.decision == SyncDecision.UPLOAD_LOCAL }
        val hasCloudNewer = actions.any { it.decision == SyncDecision.DOWNLOAD_CLOUD }

        if (conflictingPairs.isNotEmpty() || (hasLocalOnly && hasCloudNewer)) {
            val latestLocalMs = if (localFiles.isNotEmpty()) localFiles.maxOf { it.lastModified() } else System.currentTimeMillis()
            val latestCloudMs = if (cloudFiles.isNotEmpty()) cloudFiles.maxOf { Instant.parse(it.modifiedTime).toEpochMilli() } else System.currentTimeMillis()

            val choice: SyncDecision = kotlinx.coroutines.suspendCancellableCoroutine { continuation ->
                onStatus(AppStatus.Conflict(
                    localDate = formatTimestamp(latestLocalMs),
                    cloudDate = formatTimestamp(latestCloudMs),
                    gameName = displayName,
                    onResolve = { userChoice ->
                        if (continuation.isActive) {
                            continuation.resumeWith(Result.success(userChoice))
                        }
                    }
                ))
            }

            if (choice == SyncDecision.DOWNLOAD_CLOUD) {
                for (pair in pairs) {
                    pair.localFile.parentFile?.mkdirs()
                    transfer.download(token, pair.cloudFile.id, pair.localFile, pair.cloudFile.modifiedTime)
                }
            } else if (choice == SyncDecision.UPLOAD_LOCAL) {
                for (localFile in localFiles) {
                    val relativePath = localFile.toRelativeString(localSavePath).replace(File.separatorChar, '/')
                    val cloudFile = plannedCloudByPath[relativePath]
                    syncSingleFilePost(token, localFile, cloudFile, folderId, relativePath, force = true)
                }
            }
        } else {
            for (pair in pairs) {
                if (pair.decision == SyncDecision.DOWNLOAD_CLOUD) {
                    pair.localFile.parentFile?.mkdirs()
                    transfer.download(token, pair.cloudFile.id, pair.localFile, pair.cloudFile.modifiedTime)
                }
            }
            for (localFile in localFiles) {
                val relativePath = localFile.toRelativeString(localSavePath).replace(File.separatorChar, '/')
                val cloudFile = plannedCloudByPath[relativePath]
                syncSingleFilePost(token, localFile, cloudFile, folderId, relativePath, force = false)
            }
        }
    }

    private suspend fun syncFileManual(
        token: String,
        entry: GameEntry,
        localSavePath: File,
        folderId: String,
        cloudFiles: List<DriveFile>,
        displayName: String = entry.name,
    ) {
        val cloudFile = cloudFiles.find { it.name == localSavePath.name }

        val localExists = localSavePath.exists()
        val decision = planSyncDecision(
            localModifiedTimeMs = localSavePath.takeIf { localExists }?.lastModified(),
            cloudModifiedTimeIso = cloudFile?.modifiedTime,
        )

        when (decision) {
            SyncDecision.IN_SYNC -> Unit
            SyncDecision.DOWNLOAD_CLOUD -> {
                val remote = cloudFile ?: return
                localSavePath.parentFile?.mkdirs()
                transfer.download(token, remote.id, localSavePath, remote.modifiedTime)
            }
            SyncDecision.UPLOAD_LOCAL -> {
                if (!localExists) return
                val result = transfer.upload(
                    token,
                    localSavePath,
                    driveName = localSavePath.name,
                    parentFolderId = folderId,
                )
                setLocalModifiedTime(localSavePath, result.modifiedTime)
            }
            SyncDecision.CONFLICT -> {
                val remote = cloudFile ?: return
                val localDate = formatTimestamp(localSavePath.lastModified())
                val cloudDate = formatTimestamp(Instant.parse(remote.modifiedTime).toEpochMilli())

                val choice: SyncDecision = kotlinx.coroutines.suspendCancellableCoroutine { continuation ->
                    onStatus(AppStatus.Conflict(
                        localDate = localDate,
                        cloudDate = cloudDate,
                        gameName = displayName,
                        onResolve = { userChoice ->
                            if (continuation.isActive) {
                                continuation.resumeWith(Result.success(userChoice))
                            }
                        }
                    ))
                }

                when (choice) {
                    SyncDecision.DOWNLOAD_CLOUD -> transfer.download(token, remote.id, localSavePath, remote.modifiedTime)
                    SyncDecision.UPLOAD_LOCAL -> {
                        val result = transfer.update(token, remote.id, localSavePath)
                        setLocalModifiedTime(localSavePath, result.modifiedTime)
                    }
                    else -> Unit
                }
            }
        }
    }

    private fun setLocalModifiedTime(file: File, modifiedTime: String) {
        try {
            file.setLastModified(Instant.parse(modifiedTime).toEpochMilli())
        } catch (_: Exception) {
            // Drive may return a timestamp format the local filesystem cannot apply.
        }
    }

    /**
     * Full lifecycle for a game click.
     *
     * @param item The [GameItem] the user clicked (game card).
     * @param onRequestSaveSetup Suspend callback invoked if the game ends but has no configured saves.
     */
    suspend fun playWithSync(
        item: GameItem,
        onRequestSaveSetup: suspend (item: GameItem, notice: String?, cloudFileNames: List<String>) -> List<ConfiguredSaveTarget> = { _, _, _ -> emptyList() }
    ) {
        try {
            val hasDriveConfig = config.googleDrive != null
            var cachedToken: String? = null
            var forceUploadLocal = false
            var currentSyncTargets = item.effectiveSaveTargets
            val cloudPath = getCloudPathSegments(item)

            // ── Step 1: Pre-Sync (download cloud save if newer) ─────
            if (hasDriveConfig) {
                try {
                    onStatus(AppStatus.Syncing("Checking cloud saves..."))
                    cachedToken = oauthFlow.authorize(config, configManager, allowInteractive = false)

                    // Case 1: If game has no save paths configured locally, check if cloud saves exist on Drive!
                    if (currentSyncTargets.isEmpty()) {
                        val cloudFolderId = folders.findPath(cachedToken, cloudPath)
                        val cloudFiles = if (cloudFolderId != null) {
                            search.listFilesInFolder(cachedToken, cloudFolderId)
                        } else {
                            emptyList()
                        }

                        if (cloudFiles.isNotEmpty()) {
                            onStatus(AppStatus.Idle)
                            val notice = "Cloud save data was found for this game. Choose one destination folder; only these listed files will be synchronized for this game."
                            val requested = onRequestSaveSetup(item, notice, cloudFiles.map { it.name })
                            if (requested.isNotEmpty()) {
                                currentSyncTargets = requested
                            } else {
                                // User cancelled save setup prior to launch; abort launch to prevent playing without cloud saves.
                                return
                            }
                        }
                    }

                    if (currentSyncTargets.isNotEmpty()) {
                        onStatus(AppStatus.Syncing("Syncing saves for ${item.effectiveTitle}..."))
                        val targetPairs = currentSyncTargets.map { File(it.path) to it.type }
                        for (target in currentSyncTargets) {
                            val saveFile = File(target.path)
                            val reservedSiblingNames = getSiblingReservedNames(saveFile, targetPairs)
                            val decision = preSync(
                                token = cachedToken,
                                entry = item.entry,
                                localSavePath = saveFile,
                                targetType = target.type,
                                cloudPathSegments = cloudPath,
                                displayName = item.effectiveTitle,
                                reservedSiblingNames = reservedSiblingNames,
                            )
                            if (decision == SyncDecision.UPLOAD_LOCAL) {
                                forceUploadLocal = true
                            }
                        }
                    }
                } catch (_: Exception) {
                    cachedToken = null
                }
            }

            // ── Step 2: Play ────────────────────────────────────────
            onStatus(AppStatus.Playing)
            val steamAppId = when (val entry = item.entry) {
                is EmulatorSystem -> entry.steamAppId
                is NativePCGame -> entry.steamAppId
            }
            val result = runner.launch(item.entry, item.romFile, steamAppId)
            if (result.exitCode != 0) {
                onStatus(AppStatus.Error("${item.effectiveTitle} did not start or its process could not be tracked. Cloud upload was not attempted."))
                return
            }

            // ── Step 2.5: Ask for save paths if empty ───────────────
            if (currentSyncTargets.isEmpty()) {
                currentSyncTargets = onRequestSaveSetup(item, null, emptyList())
            }

            // ── Step 3: Post-Sync (upload local save if modified) ───
            if (hasDriveConfig) {
                val uploaded = uploadAfterPlay(
                    item = item,
                    syncTargets = currentSyncTargets,
                    forceUploadLocal = forceUploadLocal,
                    existingToken = cachedToken,
                )
                if (!uploaded) return
            }

            onStatus(AppStatus.Idle)

        } catch (e: Exception) {
            onStatus(AppStatus.Error("Error: ${e.message}"))
        }
    }

    private suspend fun uploadAfterPlay(
        item: GameItem,
        syncTargets: List<ConfiguredSaveTarget>,
        forceUploadLocal: Boolean,
        existingToken: String?,
        retryTargets: List<SaveUploadTarget>? = null,
        completedBeforeRetry: Int = 0,
        isRetry: Boolean = false,
        blockedFailuresBeforeRetry: List<String> = emptyList(),
    ): Boolean {
        onStatus(AppStatus.Syncing("Uploading saves for ${item.effectiveTitle}..."))
        var completedCount = completedBeforeRetry
        val failedConfiguredTargets = mutableListOf<ConfiguredSaveTarget>()
        val failedFileTargets = mutableListOf<SaveUploadTarget>()
        val blockedReasons = blockedFailuresBeforeRetry.toMutableList()
        var firstFailureReason: String? = null
        try {
            val token = existingToken ?: oauthFlow.authorize(config, configManager, allowInteractive = false)
            val cloudPath = getCloudPathSegments(item)
            for (target in retryTargets.orEmpty()) {
                if (!target.file.exists()) continue
                try {
                    val result = postSync(
                        token = token,
                        entry = item.entry,
                        localSavePath = target.file,
                        targetType = target.type,
                        cloudPathSegments = cloudPath,
                        forceAll = false,
                        driveNameOverride = target.driveName,
                        resolveCloudNewer = isRetry,
                        knownFileSiblingNames = target.reservedDriveNames,
                    )
                    completedCount += result.completedCount
                    failedFileTargets += result.failedTargets
                    blockedReasons += result.blockedReasons
                    if (firstFailureReason == null) {
                        firstFailureReason = result.failedTargets.firstNotNullOfOrNull { it.failureReason }
                    }
                } catch (e: Exception) {
                    if (e is CancellationException) throw e
                    failedFileTargets += target
                    if (firstFailureReason == null) firstFailureReason = e.message
                }
            }
            val targetPairs = syncTargets.map { File(it.path) to it.type }
            for (target in syncTargets) {
                val saveFile = File(target.path)
                if (!saveFile.exists()) continue
                val knownFileSiblingNames = syncTargets
                    .filter { it.type == SaveTargetType.FILE && it.path != target.path }
                    .mapTo(mutableSetOf()) { File(it.path).name }
                val reservedSiblingNames = getSiblingReservedNames(saveFile, targetPairs)
                try {
                    val result = postSync(
                        token = token,
                        entry = item.entry,
                        localSavePath = saveFile,
                        targetType = target.type,
                        cloudPathSegments = cloudPath,
                        forceAll = forceUploadLocal,
                        resolveCloudNewer = isRetry,
                        knownFileSiblingNames = knownFileSiblingNames,
                        reservedSiblingNames = reservedSiblingNames,
                    )
                    completedCount += result.completedCount
                    failedFileTargets += result.failedTargets
                    blockedReasons += result.blockedReasons
                    if (firstFailureReason == null) {
                        firstFailureReason = result.failedTargets.firstNotNullOfOrNull { it.failureReason }
                    }
                } catch (e: Exception) {
                    if (e is CancellationException) throw e
                    failedConfiguredTargets += target
                    if (firstFailureReason == null) firstFailureReason = e.message
                }
            }
        } catch (e: Exception) {
            if (e is CancellationException) throw e
            failedConfiguredTargets += syncTargets.filterNot { it in failedConfiguredTargets }
            failedFileTargets += retryTargets.orEmpty().filterNot { it in failedFileTargets }
            firstFailureReason = firstFailureReason ?: e.message
        }

        val distinctBlockedReasons = blockedReasons.distinct()
        if (failedConfiguredTargets.isEmpty() && failedFileTargets.isEmpty()) {
            if (distinctBlockedReasons.isNotEmpty()) {
                onStatus(AppStatus.Error(distinctBlockedReasons.joinToString("\n")))
                return false
            }
            onStatus(AppStatus.Idle)
            return true
        }

        val reason = buildList {
            firstFailureReason?.takeIf { it.isNotBlank() }?.let(::add)
            addAll(distinctBlockedReasons)
        }.distinct().joinToString("\n").ifBlank { "Unknown Drive error" }
        logger.log(Level.WARNING, "Post-game Drive sync failed for {0}: {1}", arrayOf(item.effectiveTitle, reason))
        onStatus(
            AppStatus.PostSyncFailed(
                gameName = item.effectiveTitle,
                reason = reason,
                completedSaveCount = completedCount,
                pendingSaveCount = failedConfiguredTargets.size + failedFileTargets.size + distinctBlockedReasons.size,
                onRetry = {
                    uploadAfterPlay(
                        item = item,
                        syncTargets = failedConfiguredTargets,
                        forceUploadLocal = false,
                        existingToken = null,
                        retryTargets = failedFileTargets,
                        completedBeforeRetry = completedCount,
                        isRetry = true,
                        blockedFailuresBeforeRetry = distinctBlockedReasons,
                    )
                },
            )
        )
        return false
    }

    /**
     * Pre-Sync: Check cloud save timestamp vs local. Download if cloud is newer.
     * Returns the user's resolution decision if a conflict was resolved, or null.
     */
    private suspend fun preSync(
        token: String,
        entry: GameEntry,
        localSavePath: File,
        targetType: SaveTargetType,
        cloudPathSegments: List<String> = listOf(entry.effectiveCloudFolder),
        displayName: String = entry.name,
        reservedSiblingNames: Set<String> = emptySet(),
    ): SyncDecision? {
        val folderId = folders.ensurePath(token, cloudPathSegments)
        val cloudFiles = search.listFilesInFolder(token, folderId)

        val isDirectory = isDirectorySaveTarget(targetType, localSavePath, cloudFiles, reservedSiblingNames)

        if (isDirectory) {
            val directoryCloudFiles = cloudFilesForSaveTarget(
                targetType, localSavePath, cloudFiles, reservedSiblingNames,
            )
            data class FilePair(val localFile: File, val cloudFile: DriveFile, val decision: SyncDecision)

            val pairs = directoryCloudFiles.mapNotNull { cloudFile ->
                val localFile = resolveRemoteFile(localSavePath, cloudFile.name) ?: return@mapNotNull null
                val decision = planSyncDecision(
                    localModifiedTimeMs = localFile.takeIf { it.exists() }?.lastModified(),
                    cloudModifiedTimeIso = cloudFile.modifiedTime,
                )
                FilePair(localFile, cloudFile, decision)
            }

            val conflictingPairs = pairs.filter { it.decision == SyncDecision.CONFLICT }

            if (conflictingPairs.isNotEmpty()) {
                val latestLocalMs = conflictingPairs.maxOf { it.localFile.lastModified() }
                val latestCloudMs = conflictingPairs.maxOf { Instant.parse(it.cloudFile.modifiedTime).toEpochMilli() }

                val choice: SyncDecision = kotlinx.coroutines.suspendCancellableCoroutine { continuation ->
                    onStatus(AppStatus.Conflict(
                        localDate = formatTimestamp(latestLocalMs),
                        cloudDate = formatTimestamp(latestCloudMs),
                        gameName = displayName,
                        onResolve = { userChoice ->
                            if (continuation.isActive) {
                                continuation.resumeWith(Result.success(userChoice))
                            }
                        }
                    ))
                }

                if (choice == SyncDecision.DOWNLOAD_CLOUD) {
                    for (pair in pairs) {
                        pair.localFile.parentFile?.mkdirs()
                        transfer.download(token, pair.cloudFile.id, pair.localFile, pair.cloudFile.modifiedTime)
                    }
                }
                return choice
            } else {
                for (pair in pairs) {
                    if (pair.decision == SyncDecision.DOWNLOAD_CLOUD) {
                        pair.localFile.parentFile?.mkdirs()
                        transfer.download(token, pair.cloudFile.id, pair.localFile, pair.cloudFile.modifiedTime)
                    }
                }
                return null
            }
        } else {
            val cloudFile = cloudFiles.find { it.name == localSavePath.name } ?: return null
            val plannedDecision = planSyncDecision(
                localModifiedTimeMs = localSavePath.takeIf { it.exists() }?.lastModified(),
                cloudModifiedTimeIso = cloudFile.modifiedTime,
            )

            when (plannedDecision) {
                SyncDecision.DOWNLOAD_CLOUD -> {
                    localSavePath.parentFile?.mkdirs()
                    transfer.download(token, cloudFile.id, localSavePath, cloudFile.modifiedTime)
                    return null
                }
                SyncDecision.UPLOAD_LOCAL, SyncDecision.IN_SYNC -> {
                    return null
                }
                SyncDecision.CONFLICT -> {
                    val localDate = formatTimestamp(localSavePath.lastModified())
                    val cloudDate = formatTimestamp(Instant.parse(cloudFile.modifiedTime).toEpochMilli())

                    val choice: SyncDecision = kotlinx.coroutines.suspendCancellableCoroutine { continuation ->
                        onStatus(AppStatus.Conflict(
                            localDate = localDate,
                            cloudDate = cloudDate,
                            gameName = displayName,
                            onResolve = { userChoice ->
                                if (continuation.isActive) {
                                    continuation.resumeWith(Result.success(userChoice))
                                }
                            }
                        ))
                    }
                    if (choice == SyncDecision.DOWNLOAD_CLOUD) {
                        transfer.download(token, cloudFile.id, localSavePath, cloudFile.modifiedTime)
                    }
                    return choice
                }
            }
        }
    }

    /**
     * Post-Sync: Upload local save to the entry's subfolder on Drive.
     */
    private suspend fun postSync(
        token: String,
        entry: GameEntry,
        localSavePath: File,
        targetType: SaveTargetType,
        cloudPathSegments: List<String> = listOf(entry.effectiveCloudFolder),
        forceAll: Boolean = false,
        driveNameOverride: String? = null,
        resolveCloudNewer: Boolean = false,
        knownFileSiblingNames: Set<String> = emptySet(),
        reservedSiblingNames: Set<String> = emptySet(),
    ): PostSyncResult {
        if (!localSavePath.exists()) return PostSyncResult(0, emptyList())

        val driveName = driveNameOverride ?: localSavePath.name
        if (targetType == SaveTargetType.FILE && driveName in knownFileSiblingNames) {
            return PostSyncResult(
                0,
                emptyList(),
                listOf("The save name '$driveName' is already used by another configured file target."),
            )
        }

        val folderId = folders.ensurePath(token, cloudPathSegments)
        val cloudFiles = search.listFilesInFolder(token, folderId)
        var completedCount = 0
        val failedTargets = mutableListOf<SaveUploadTarget>()
        val blockedReasons = mutableListOf<String>()

        val allReservedNames = knownFileSiblingNames + reservedSiblingNames

        // Legacy paths that did not exist during config migration remain UNKNOWN until
        // the game creates them. Resolve from the filesystem after the game exits.
        val isDirectory = isDirectorySaveTarget(targetType, localSavePath, cloudFiles, allReservedNames)
        if (isDirectory) {
            val localFiles = localSavePath.walkTopDown().filter { it.isFile }.toList()
            for (localFile in localFiles) {
                val relativePath = localFile.toRelativeString(localSavePath).replace(File.separatorChar, '/')
                if (relativePath in knownFileSiblingNames) {
                    blockedReasons += "The directory save '$relativePath' conflicts with another configured file target. Rename or reconfigure one of the saves before syncing it."
                    continue
                }
                if (relativePath in reservedSiblingNames) {
                    continue
                }
                val cloudFile = cloudFiles.find { it.name == relativePath }
                try {
                    syncSingleFilePost(
                        token, localFile, cloudFile, folderId, relativePath,
                        force = forceAll,
                        resolveCloudNewer = resolveCloudNewer,
                        displayName = entry.name,
                    )
                    completedCount++
                } catch (e: Exception) {
                    if (e is CancellationException) throw e
                    failedTargets += SaveUploadTarget(
                        localFile, SaveTargetType.FILE, relativePath, e.message, allReservedNames,
                    )
                }
            }
        } else {
            val cloudFile = cloudFiles.find { it.name == driveName }
            try {
                syncSingleFilePost(
                    token, localSavePath, cloudFile, folderId, driveName,
                    force = forceAll,
                    resolveCloudNewer = resolveCloudNewer,
                    displayName = entry.name,
                )
                completedCount++
            } catch (e: Exception) {
                if (e is CancellationException) throw e
                failedTargets += SaveUploadTarget(
                    localSavePath, SaveTargetType.FILE, driveName, e.message, allReservedNames,
                )
            }
        }
        return PostSyncResult(completedCount, failedTargets, blockedReasons)
    }

    private suspend fun syncSingleFilePost(
        token: String,
        localFile: File,
        cloudFile: DriveFile?,
        parentFolderId: String,
        driveName: String,
        force: Boolean = false,
        resolveCloudNewer: Boolean = false,
        displayName: String = driveName,
    ) {
        if (cloudFile != null) {
            val decision = planSyncDecision(
                localModifiedTimeMs = localFile.lastModified(),
                cloudModifiedTimeIso = cloudFile.modifiedTime,
                localNewerAction = LocalNewerAction.UPLOAD,
            )

            when {
                force || decision == SyncDecision.UPLOAD_LOCAL -> {
                    val res = transfer.update(token, cloudFile.id, localFile)
                    try {
                        localFile.setLastModified(Instant.parse(res.modifiedTime).toEpochMilli())
                    } catch (_: Exception) {
                    }
                }
                decision == SyncDecision.DOWNLOAD_CLOUD && resolveCloudNewer -> {
                    val selectedDecision = kotlinx.coroutines.suspendCancellableCoroutine<SyncDecision> { continuation ->
                        onStatus(AppStatus.Conflict(
                            localDate = formatTimestamp(localFile.lastModified()),
                            cloudDate = formatTimestamp(Instant.parse(cloudFile.modifiedTime).toEpochMilli()),
                            gameName = displayName,
                            onResolve = { choice ->
                                if (continuation.isActive) continuation.resumeWith(Result.success(choice))
                            },
                        ))
                    }
                    when (selectedDecision) {
                        SyncDecision.UPLOAD_LOCAL -> {
                            val res = transfer.update(token, cloudFile.id, localFile)
                            runCatching { localFile.setLastModified(Instant.parse(res.modifiedTime).toEpochMilli()) }
                        }
                        SyncDecision.DOWNLOAD_CLOUD -> transfer.download(token, cloudFile.id, localFile, cloudFile.modifiedTime)
                        else -> Unit
                    }
                }
            }
        } else {
            val res = transfer.upload(token, localFile, driveName = driveName, parentFolderId = parentFolderId)
            try {
                localFile.setLastModified(Instant.parse(res.modifiedTime).toEpochMilli())
            } catch (_: Exception) {
            }
        }
    }

    companion object {
        private val dateFormatter = DateTimeFormatter
            .ofPattern("yyyy-MM-dd HH:mm:ss")
            .withZone(ZoneId.systemDefault())

        fun formatTimestamp(epochMs: Long): String =
            dateFormatter.format(Instant.ofEpochMilli(epochMs))
    }
}
