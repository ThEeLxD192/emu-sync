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
import java.io.File
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.logging.Level
import java.util.logging.Logger

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

                for (target in folderTargets) {
                    val savePath = target.file
                    val isDirectory = target.type == SaveTargetType.DIRECTORY

                    if (isDirectory) {
                        val localFiles = if (savePath.exists() && savePath.isDirectory) {
                            savePath.walkTopDown().filter { it.isFile }.toList()
                        } else {
                            emptyList()
                        }

                        if (localFiles.isEmpty() && cloudFiles.isEmpty()) continue
                        anyFileChecked = true

                        for (cloudFile in cloudFiles) {
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
                            if (cloudFiles.none { it.name == relPath }) {
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

                for (target in folderTargets) {
                    val savePath = target.file
                    onStatus(AppStatus.Syncing("Syncing ${target.displayName}..."))
                    val isDirectory = target.type == SaveTargetType.DIRECTORY

                    if (isDirectory) {
                        syncDirectoryManual(token, entry, savePath, folderId, cloudFiles, displayName = target.displayName)
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

                    var hasConflict = false
                    var hasNewerLocal = false
                    var hasNewerCloud = false

                    for (target in localSaveTargets) {
                        val savePath = File(target.path)
                        val isDirectory = target.type == SaveTargetType.DIRECTORY

                        if (isDirectory) {
                            val localDirFiles = if (savePath.exists() && savePath.isDirectory) {
                                savePath.walkTopDown().filter { it.isFile }.toList()
                            } else emptyList()

                            for (cloudFile in cloudFiles) {
                                val localF = resolveRemoteFile(savePath, cloudFile.name) ?: continue
                                if (!localF.exists()) {
                                    hasNewerCloud = true
                                } else {
                                    val decision = resolveConflict(localF.lastModified(), cloudFile.modifiedTime)
                                    when (decision) {
                                        SyncDecision.CONFLICT -> hasConflict = true
                                        SyncDecision.DOWNLOAD_CLOUD -> hasNewerCloud = true
                                        SyncDecision.UPLOAD_LOCAL -> hasNewerLocal = true
                                        SyncDecision.IN_SYNC -> {}
                                    }
                                }
                            }
                            for (localF in localDirFiles) {
                                val rel = localF.toRelativeString(savePath).replace(File.separatorChar, '/')
                                if (cloudFiles.none { it.name == rel }) {
                                    hasNewerLocal = true
                                }
                            }
                        } else {
                            val cloudFile = cloudFiles.find { it.name == savePath.name }
                            val localExists = savePath.exists()
                            val cloudExists = cloudFile != null

                            if (!localExists && cloudExists) {
                                hasNewerCloud = true
                            } else if (localExists && !cloudExists) {
                                hasNewerLocal = true
                            } else if (localExists && cloudFile != null) {
                                val decision = resolveConflict(savePath.lastModified(), cloudFile.modifiedTime)
                                when (decision) {
                                    SyncDecision.CONFLICT -> hasConflict = true
                                    SyncDecision.DOWNLOAD_CLOUD -> hasNewerCloud = true
                                    SyncDecision.UPLOAD_LOCAL -> hasNewerLocal = true
                                    SyncDecision.IN_SYNC -> {}
                                }
                            }
                        }
                    }

                    if (hasConflict || (hasNewerLocal && hasNewerCloud)) {
                        val allLocalFiles = localSaveTargets.flatMap { target ->
                            val path = File(target.path)
                            if (target.type == SaveTargetType.DIRECTORY && path.exists()) path.walkTopDown().filter { it.isFile }.toList()
                            else if (path.exists()) listOf(path) else emptyList()
                        }
                        val latestLocalMs = if (allLocalFiles.isNotEmpty()) allLocalFiles.maxOf { it.lastModified() } else System.currentTimeMillis()
                        val latestCloudMs = if (cloudFiles.isNotEmpty()) cloudFiles.maxOf { Instant.parse(it.modifiedTime).toEpochMilli() } else System.currentTimeMillis()

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
                                val isDirectory = target.type == SaveTargetType.DIRECTORY
                                if (isDirectory) {
                                    for (cloudFile in cloudFiles) {
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
                                if (target.type == SaveTargetType.DIRECTORY) {
                                    val localFiles = savePath.walkTopDown().filter { it.isFile }.toList()
                                    for (localFile in localFiles) {
                                        val relPath = localFile.toRelativeString(savePath).replace(File.separatorChar, '/')
                                        val cloudFile = cloudFiles.find { it.name == relPath }
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
                            val isDirectory = target.type == SaveTargetType.DIRECTORY
                            if (isDirectory) {
                                for (cloudFile in cloudFiles) {
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
                            if (target.type == SaveTargetType.DIRECTORY) {
                                val localFiles = savePath.walkTopDown().filter { it.isFile }.toList()
                                for (localFile in localFiles) {
                                    val relPath = localFile.toRelativeString(savePath).replace(File.separatorChar, '/')
                                    val cloudFile = cloudFiles.find { it.name == relPath }
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
    ) {
        data class FilePair(val localFile: File, val cloudFile: DriveFile, val decision: SyncDecision)

        val pairs = cloudFiles.mapNotNull { cloudFile ->
            val localFile = resolveRemoteFile(localSavePath, cloudFile.name) ?: return@mapNotNull null
            val decision = planSyncDecision(
                localModifiedTimeMs = localFile.takeIf { it.exists() }?.lastModified(),
                cloudModifiedTimeIso = cloudFile.modifiedTime,
            )
            FilePair(localFile, cloudFile, decision)
        }

        val localFiles = if (localSavePath.exists() && localSavePath.isDirectory) {
            localSavePath.walkTopDown().filter { it.isFile }.toList()
        } else {
            emptyList()
        }

        val conflictingPairs = pairs.filter { it.decision == SyncDecision.CONFLICT }
        val hasLocalOnly = localFiles.any { local ->
            val rel = local.toRelativeString(localSavePath).replace(File.separatorChar, '/')
            cloudFiles.none { it.name == rel } &&
                planSyncDecision(local.lastModified(), null) == SyncDecision.UPLOAD_LOCAL
        }
        val hasCloudNewer = pairs.any { it.decision == SyncDecision.DOWNLOAD_CLOUD }

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
                    val cloudFile = cloudFiles.find { it.name == relativePath }
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
                val cloudFile = cloudFiles.find { it.name == relativePath }
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
                        for (target in currentSyncTargets) {
                            val saveFile = File(target.path)
                            val decision = preSync(
                                token = cachedToken,
                                entry = item.entry,
                                localSavePath = saveFile,
                                targetType = target.type,
                                cloudPathSegments = cloudPath,
                                displayName = item.effectiveTitle,
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

            // ── Step 2.5: Ask for save paths if empty ───────────────
            if (currentSyncTargets.isEmpty() && result.exitCode == 0) {
                currentSyncTargets = onRequestSaveSetup(item, null, emptyList())
            }

            // ── Step 3: Post-Sync (upload local save if modified) ───
            if (hasDriveConfig && result.exitCode == 0) {
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
        syncTargets: List<com.emusync.model.SaveTarget>,
        forceUploadLocal: Boolean,
        existingToken: String?,
    ): Boolean {
        return try {
            onStatus(AppStatus.Syncing("Uploading save for ${item.effectiveTitle}..."))
            val token = existingToken ?: oauthFlow.authorize(config, configManager, allowInteractive = false)
            val cloudPath = getCloudPathSegments(item)
            for (target in syncTargets) {
                val saveFile = File(target.path)
                if (saveFile.exists()) {
                    postSync(
                        token = token,
                        entry = item.entry,
                        localSavePath = saveFile,
                        targetType = target.type,
                        cloudPathSegments = cloudPath,
                        forceAll = forceUploadLocal,
                    )
                }
            }
            onStatus(AppStatus.Idle)
            true
        } catch (e: Exception) {
            val reason = e.message?.takeIf { it.isNotBlank() } ?: "Unknown Drive error"
            logger.log(Level.WARNING, "Post-game Drive sync failed for {0}: {1}", arrayOf(item.effectiveTitle, reason))
            onStatus(
                AppStatus.PostSyncFailed(
                    gameName = item.effectiveTitle,
                    reason = reason,
                    onRetry = {
                        uploadAfterPlay(
                            item = item,
                            syncTargets = syncTargets,
                            forceUploadLocal = forceUploadLocal,
                            existingToken = null,
                        )
                    },
                )
            )
            false
        }
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
    ): SyncDecision? {
        val folderId = folders.ensurePath(token, cloudPathSegments)
        val cloudFiles = search.listFilesInFolder(token, folderId)

        val isDirectory = targetType == SaveTargetType.DIRECTORY

        if (isDirectory) {
            data class FilePair(val localFile: File, val cloudFile: DriveFile, val decision: SyncDecision)

            val pairs = cloudFiles.mapNotNull { cloudFile ->
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
    ) {
        if (!localSavePath.exists()) return

        val folderId = folders.ensurePath(token, cloudPathSegments)
        val cloudFiles = search.listFilesInFolder(token, folderId)

        if (targetType == SaveTargetType.DIRECTORY) {
            val localFiles = localSavePath.walkTopDown().filter { it.isFile }.toList()
            for (localFile in localFiles) {
                val relativePath = localFile.toRelativeString(localSavePath).replace(File.separatorChar, '/')
                val cloudFile = cloudFiles.find { it.name == relativePath }
                syncSingleFilePost(token, localFile, cloudFile, folderId, relativePath, force = forceAll)
            }
        } else {
            val cloudFile = cloudFiles.find { it.name == localSavePath.name }
            syncSingleFilePost(token, localSavePath, cloudFile, folderId, localSavePath.name, force = forceAll)
        }
    }

    private suspend fun syncSingleFilePost(
        token: String,
        localFile: File,
        cloudFile: DriveFile?,
        parentFolderId: String,
        driveName: String,
        force: Boolean = false
    ) {
        if (cloudFile != null) {
            val decision = planSyncDecision(
                localModifiedTimeMs = localFile.lastModified(),
                cloudModifiedTimeIso = cloudFile.modifiedTime,
                localNewerAction = LocalNewerAction.UPLOAD,
            )

            if (force || decision == SyncDecision.UPLOAD_LOCAL) {
                val res = transfer.update(token, cloudFile.id, localFile)
                try {
                    localFile.setLastModified(Instant.parse(res.modifiedTime).toEpochMilli())
                } catch (_: Exception) {
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
