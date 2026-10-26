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
    cloudName: String? = null,
): Boolean {
    if (path.exists()) {
        return path.isDirectory
    }
    return when (type) {
        SaveTargetType.DIRECTORY -> true
        SaveTargetType.FILE -> false
        SaveTargetType.UNKNOWN -> cloudFiles.none { cloudFile ->
            cloudFile.name == (cloudName ?: path.name) && cloudFile.name !in reservedSiblingNames
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
    private val trackedSync = TrackedFileSync(transfer, SyncHistoryStore(configManager.configFile))
    private val folderMigrator = CloudFolderMigrator(folders, search)
    private val oauthFlow = OAuthFlow(client)
    private val runner = GameRunner()

    data class SaveTarget(
        val file: File,
        val type: SaveTargetType,
        val cloudPathSegments: List<String>,
        val displayName: String,
        val cloudPath: String? = null,
    ) {
        fun configured() = ConfiguredSaveTarget(file.path, type, cloudPath)
    }

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
                    .map { normalizeSavePath(it.path) to it.cloudPath }
                    .toSet()
                val romPaths = romName?.let { name ->
                    entry.effectiveSaveTargetsForRom(name)
                }.orEmpty()
                val hasDistinctRomPath = romPaths.any { (normalizeSavePath(it.path) to it.cloudPath) !in globalPaths }

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
                    SaveTarget(File(it.path), it.type, cloudPath, entry.name, it.cloudPath)
                }
            }
            is EmulatorSystem -> {
                val targets = mutableListOf<SaveTarget>()

                // 1. Global emulator save paths (if any)
                if (entry.effectiveSaveTargets().isNotEmpty()) {
                    val globalPath = listOf(entry.effectiveCloudFolder)
                    for (target in entry.effectiveSaveTargets()) {
                        targets.add(SaveTarget(File(target.path), target.type, globalPath, entry.name, target.cloudPath))
                    }
                }

                // 2. Per-ROM save paths
                val romNames = entry.saveTargetsByRom.keys + entry.savePathsByRom.keys
                for (romName in romNames) {
                    val title = entry.getEffectiveTitle(romName, romName)
                    val romCloudPath = listOf(entry.effectiveCloudFolder, sanitizeCloudFolderName(title))
                    for (target in entry.effectiveSaveTargetsForRom(romName)) {
                        targets.add(SaveTarget(File(target.path), target.type, romCloudPath, title, target.cloudPath))
                    }
                }

                targets.distinctBy { it.file.absolutePath to it.cloudPath }
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
                val folderId = folders.findPath(token, cloudPath)
                val cloudFiles = if (folderId != null) {
                    canonicalCloudFiles(search.listFilesInFolder(token, folderId))
                } else {
                    emptyList()
                }

                val configuredTargets = folderTargets.map { it.configured() }
                validateSaveTargetMappings(configuredTargets, cloudFiles)

                for (target in folderTargets) {
                    val savePath = target.file
                    val reservedSiblingNames = reservedCloudNames(target.configured(), configuredTargets, cloudFiles)
                    val isDirectory = target.configured().isDirectory(cloudFiles, configuredTargets)

                    if (isDirectory) {
                        val targetCloudFiles = cloudFilesForSaveTarget(
                            target.type, savePath, projectDirectoryFiles(cloudFiles, target.cloudPath), reservedSiblingNames,
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
                            val decision = trackedSync.plan(localFile, cloudFile, folderId.orEmpty(),
                                directoryCloudName(target.cloudPath, cloudFile.name))
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
                                hasNewerLocal = true
                            }
                        }
                    } else {
                        val cloudFile = cloudFiles.find { it.name == fileCloudName(target.configured()) }
                        val localExists = savePath.exists()
                        val cloudExists = cloudFile != null

                        val decision = trackedSync.plan(savePath, cloudFile, folderId.orEmpty(), fileCloudName(target.configured()))
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
            if (e is CancellationException) throw e
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
                val cloudFiles = canonicalCloudFiles(search.listFilesInFolder(token, folderId))
                val configuredTargets = folderTargets.map { it.configured() }
                validateSaveTargetMappings(configuredTargets, cloudFiles)

                for (target in folderTargets) {
                    val savePath = target.file
                    onStatus(AppStatus.Syncing("Syncing ${target.displayName}..."))
                    val reservedSiblingNames = reservedCloudNames(target.configured(), configuredTargets, cloudFiles)
                    val isDirectory = target.configured().isDirectory(cloudFiles, configuredTargets)

                    if (isDirectory) {
                        val targetCloudFiles = cloudFilesForSaveTarget(
                            target.type, savePath, projectDirectoryFiles(cloudFiles, target.cloudPath), reservedSiblingNames,
                        )
                        syncDirectoryManual(
                            token, entry, savePath, folderId, targetCloudFiles,
                            displayName = target.displayName,
                            reservedSiblingNames = reservedSiblingNames,
                            cloudPrefix = target.cloudPath,
                        )
                    } else {
                        syncFileManual(token, entry, savePath, folderId, cloudFiles, displayName = target.displayName,
                            driveName = fileCloudName(target.configured()))
                    }
                }
            }

            onStatus(AppStatus.Idle)
            true
        } catch (e: Exception) {
            if (e is CancellationException) throw e
            onStatus(AppStatus.Error("Sync error: ${e.message}"))
            false
        }
    }

    /**
     * Handles renaming a game's cloud folder on Google Drive when the user changes its title.
     *
     * 1. If old folder exists and new folder does not: renames/moves the old folder to the new name.
     * 2. If new folder already exists (e.g. from another device):
     *    - Refuses duplicate names and moves non-conflicting files with rollback on failure.
     *    - Keeps the old folder as a recovery location.
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
                if (e is CancellationException) throw e
                onStatus(AppStatus.Error("Drive authentication failed: ${e.message}"))
                return false
            }

            val oldFolderId = folders.findPath(token, oldCloudPath)
            val newFolderId = folders.findPath(token, newCloudPath)

            when {
                // Case 1: Old folder exists, but new folder does NOT exist yet -> Rename old folder!
                oldFolderId != null && newFolderId == null -> {
                    onStatus(AppStatus.Syncing("Renaming cloud folder to $newTitle..."))
                    folderMigrator.migrate(token, oldCloudPath, newCloudPath)
                    onStatus(AppStatus.Idle)
                    true
                }

                // Case 2: New folder ALREADY exists! (with or without old folder) -> Check conflict and let user decide!
                newFolderId != null -> {
                    onStatus(AppStatus.Syncing("Checking cloud files in $newTitle..."))

                    // Preflight duplicates before moving anything or changing local saves.
                    if (oldFolderId != null && oldFolderId != newFolderId) {
                        folderMigrator.migrate(token, oldCloudPath, newCloudPath)
                    }

                    // Now check conflict between local save paths and newFolderId
                    val cloudFiles = search.listFilesInFolder(token, newFolderId)
                    validateSaveTargetMappings(localSaveTargets, cloudFiles)

                    for (target in localSaveTargets) {
                        val savePath = File(target.path)
                        val reserved = reservedCloudNames(target, localSaveTargets, cloudFiles)
                        if (target.isDirectory(cloudFiles, localSaveTargets)) {
                            syncDirectoryManual(token, entry, savePath, newFolderId,
                                cloudFilesForSaveTarget(target.type, savePath,
                                    projectDirectoryFiles(cloudFiles, target.cloudPath), reserved),
                                newTitle, reserved, target.cloudPath)
                        } else {
                            syncFileManual(token, entry, savePath, newFolderId, cloudFiles,
                                newTitle, fileCloudName(target))
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
            if (e is CancellationException) throw e
            onStatus(AppStatus.Error("Cloud rename error: ${e.message}"))
            false
        }
    }

    /** Migrates only this entry's folder, using the configuration before the edit. */
    suspend fun renameEntryCloudData(
        oldEntry: GameEntry,
        newEntry: GameEntry,
    ): Boolean {
        fun entryPath(entry: GameEntry): List<String> = when (entry) {
            is NativePCGame -> getCloudPathSegments(entry)
            is EmulatorSystem -> listOf(entry.effectiveCloudFolder)
        }
        val oldPath = entryPath(oldEntry)
        val newPath = entryPath(newEntry)
        if (oldPath == newPath) return true

        val entryIndex = config.entries.indexOfFirst { it === oldEntry }
            .takeIf { it >= 0 } ?: config.entries.indexOf(oldEntry)
        val shared = config.entries.filterIndexed { index, _ -> index != entryIndex }.any { other ->
            entryPath(other) == oldPath ||
                (oldPath.size == 1 && other.effectiveCloudFolder == oldPath.first()) ||
                getSaveTargets(other).any { it.cloudPathSegments == oldPath }
        }
        if (shared) {
            onStatus(AppStatus.Error(
                "The cloud folder '${oldPath.joinToString("/")}' is shared with another entry. " +
                    "Keep the existing cloud folder when editing this entry to protect their saves.",
            ))
            return false
        }

        val driveConfig = config.googleDrive ?: return true
        if (driveConfig.refreshToken.isNullOrBlank()) return true

        return try {
            onStatus(AppStatus.Syncing("Moving cloud saves for ${newEntry.name}..."))
            val token = oauthFlow.authorize(config, configManager, allowInteractive = false)
            folderMigrator.migrate(token, oldPath, newPath)
            onStatus(AppStatus.Idle)
            true
        } catch (e: Exception) {
            if (e is CancellationException) throw e
            logger.log(Level.WARNING, "Cloud entry migration failed", e)
            onStatus(AppStatus.Error("Cloud rename error: ${e.message}"))
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
        cloudPrefix: String? = null,
        beforePlay: Boolean = false,
    ) {
        val localFiles = if (localSavePath.isDirectory) localSavePath.walkTopDown().filter {
            it.isFile && it.toRelativeString(localSavePath).replace(File.separatorChar, '/') !in reservedSiblingNames
        }.associateBy { it.toRelativeString(localSavePath).replace(File.separatorChar, '/') } else emptyMap()
        val remoteFiles = canonicalCloudFiles(cloudFiles)
            .filter { resolveRemoteFile(localSavePath, it.name) != null }.associateBy { it.name }
        data class Action(val local: File, val remote: DriveFile?, val name: String, val decision: SyncDecision)
        val actions = (localFiles.keys + remoteFiles.keys).sorted().map { name ->
            val local = localFiles[name] ?: resolveRemoteFile(localSavePath, name)!!
            val remote = remoteFiles[name]
            val remoteName = directoryCloudName(cloudPrefix, name)
            Action(local, remote, remoteName, trackedSync.plan(local, remote, folderId, remoteName))
        }
        val conflict = actions.firstOrNull { it.decision == SyncDecision.CONFLICT }
        val mixed = actions.any { it.decision == SyncDecision.UPLOAD_LOCAL } &&
            actions.any { it.decision == SyncDecision.DOWNLOAD_CLOUD }
        val choice = if (conflict != null || mixed) {
            val representative = conflict ?: actions.first { it.remote != null }
            requestFileConflict(representative.local, representative.remote!!, displayName)
        } else null
        for (action in actions) {
            val decision = when {
                action.decision == SyncDecision.IN_SYNC -> SyncDecision.IN_SYNC
                choice == SyncDecision.DOWNLOAD_CLOUD -> if (action.remote != null) choice else null
                choice == SyncDecision.UPLOAD_LOCAL -> if (action.local.isFile) choice else null
                else -> action.decision
            }
            if (decision != null && !(beforePlay && choice == null && decision == SyncDecision.UPLOAD_LOCAL)) {
                applyFileDecision(token, action.local, action.remote, folderId, action.name, decision)
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
        driveName: String = localSavePath.name,
        beforePlay: Boolean = false,
    ) {
        val remote = canonicalCloudFiles(cloudFiles).find { it.name == driveName }
        val decision = trackedSync.plan(localSavePath, remote, folderId, driveName)
        val choice = if (decision == SyncDecision.CONFLICT && remote != null) {
            requestFileConflict(localSavePath, remote, displayName)
        } else decision
        if (!(beforePlay && decision == SyncDecision.UPLOAD_LOCAL)) {
            applyFileDecision(token, localSavePath, remote, folderId, driveName, choice)
        }
    }

    private suspend fun requestFileConflict(local: File, remote: DriveFile, displayName: String): SyncDecision =
        kotlinx.coroutines.suspendCancellableCoroutine { continuation ->
            onStatus(AppStatus.Conflict(
                localDate = formatTimestamp(local.lastModified()),
                cloudDate = formatTimestamp(Instant.parse(remote.modifiedTime).toEpochMilli()),
                gameName = displayName,
                onResolve = { choice -> if (continuation.isActive) continuation.resumeWith(Result.success(choice)) },
            ))
        }

    private suspend fun applyFileDecision(
        token: String, local: File, remote: DriveFile?, folderId: String, driveName: String, decision: SyncDecision,
    ) {
        when (decision) {
            SyncDecision.DOWNLOAD_CLOUD -> remote?.let { trackedSync.download(token, it, local, folderId, driveName) }
            SyncDecision.UPLOAD_LOCAL -> if (local.isFile) trackedSync.upload(token, local, remote, folderId, driveName)
            SyncDecision.IN_SYNC -> if (local.isFile && remote != null) trackedSync.remember(local, remote, folderId, driveName)
            SyncDecision.CONFLICT -> error("A conflict must be resolved before applying a save")
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
        resolveConfiguredItem: (GameItem) -> GameItem = { it },
        onRequestSaveSetup: suspend (item: GameItem, notice: String?, cloudFileNames: List<String>) -> List<ConfiguredSaveTarget> = { _, _, _ -> emptyList() }
    ) {
        try {
            val hasDriveConfig = config.googleDrive != null
            var currentItem = item
            var currentSyncTargets = currentItem.effectiveSaveTargets
            var cloudPath = getCloudPathSegments(currentItem)

            // ── Step 1: Pre-Sync (download cloud save if newer) ─────
            if (hasDriveConfig) {
                try {
                    onStatus(AppStatus.Syncing("Checking cloud saves..."))
                    val token = oauthFlow.authorize(config, configManager, allowInteractive = false)

                    // Case 1: If game has no save paths configured locally, check if cloud saves exist on Drive!
                    if (currentSyncTargets.isEmpty()) {
                        val cloudFolderId = folders.findPath(token, cloudPath)
                        val cloudFiles = if (cloudFolderId != null) {
                            search.listFilesInFolder(token, cloudFolderId)
                        } else {
                            emptyList()
                        }

                        if (cloudFiles.isNotEmpty()) {
                            onStatus(AppStatus.Idle)
                            val notice = "Cloud save data was found for this game. Choose one destination folder; only these listed files will be synchronized for this game."
                            val requested = onRequestSaveSetup(currentItem, notice, cloudFiles.map { it.name })
                            if (requested.isNotEmpty()) {
                                currentSyncTargets = requested
                                currentItem = resolveConfiguredItem(currentItem)
                                cloudPath = getCloudPathSegments(currentItem)
                            } else {
                                // User cancelled save setup prior to launch; abort launch to prevent playing without cloud saves.
                                return
                            }
                        }
                    }

                    if (currentSyncTargets.isNotEmpty()) {
                        onStatus(AppStatus.Syncing("Syncing saves for ${currentItem.effectiveTitle}..."))
                        val folderId = folders.ensurePath(token, cloudPath)
                        val cloudFiles = canonicalCloudFiles(search.listFilesInFolder(token, folderId))
                        validateSaveTargetMappings(currentSyncTargets, cloudFiles)
                        for (target in currentSyncTargets) {
                            val saveFile = File(target.path)
                            val reservedSiblingNames = reservedCloudNames(target, currentSyncTargets, cloudFiles)
                            preSync(
                                token = token,
                                entry = currentItem.entry,
                                localSavePath = saveFile,
                                targetType = target.type,
                                cloudPathSegments = cloudPath,
                                displayName = currentItem.effectiveTitle,
                                reservedSiblingNames = reservedSiblingNames,
                                cloudPath = target.cloudPath,
                            )
                        }
                    }
                } catch (e: Exception) {
                    if (e is CancellationException || e is SaveTargetMappingException || e is SaveSyncException) throw e
                }
            }

            // ── Step 2: Play ────────────────────────────────────────
            onStatus(AppStatus.Playing)
            val steamAppId = when (val entry = currentItem.entry) {
                is EmulatorSystem -> entry.steamAppId
                is NativePCGame -> entry.steamAppId
            }
            val result = runner.launch(currentItem.entry, currentItem.romFile, steamAppId)
            if (result.exitCode != 0) {
                onStatus(AppStatus.Error("${currentItem.effectiveTitle} did not start or its process could not be tracked. Cloud upload was not attempted."))
                return
            }

            // ── Step 2.5: Ask for save paths if empty ───────────────
            if (currentSyncTargets.isEmpty()) {
                currentSyncTargets = onRequestSaveSetup(currentItem, null, emptyList())
                currentItem = resolveConfiguredItem(currentItem)
            }

            // ── Step 3: Post-Sync (upload local save if modified) ───
            if (hasDriveConfig) {
                val uploaded = uploadAfterPlay(
                    item = currentItem,
                    syncTargets = currentSyncTargets,
                )
                if (!uploaded) return
            }

            onStatus(AppStatus.Idle)

        } catch (e: Exception) {
            if (e is CancellationException) throw e
            onStatus(AppStatus.Error("Error: ${e.message}"))
        }
    }

    private suspend fun uploadAfterPlay(
        item: GameItem,
        syncTargets: List<ConfiguredSaveTarget>,
        retryTargets: List<SaveUploadTarget>? = null,
        completedBeforeRetry: Int = 0,
        blockedFailuresBeforeRetry: List<String> = emptyList(),
        mappingTargets: List<ConfiguredSaveTarget> = syncTargets,
    ): Boolean {
        onStatus(AppStatus.Syncing("Uploading saves for ${item.effectiveTitle}..."))
        var completedCount = completedBeforeRetry
        val failedConfiguredTargets = mutableListOf<ConfiguredSaveTarget>()
        val failedFileTargets = mutableListOf<SaveUploadTarget>()
        val blockedReasons = blockedFailuresBeforeRetry.toMutableList()
        var firstFailureReason: String? = null
        try {
            // Games can outlive an access token. Always authorize again before post-play uploads.
            val token = oauthFlow.authorize(config, configManager, allowInteractive = false)
            val cloudPath = getCloudPathSegments(item)
            val folderId = folders.ensurePath(token, cloudPath)
            val cloudFiles = canonicalCloudFiles(search.listFilesInFolder(token, folderId))
            validateSaveTargetMappings(mappingTargets, cloudFiles)
            for (target in retryTargets.orEmpty()) {
                if (!target.file.exists()) continue
                try {
                    val result = postSync(
                        token = token,
                        entry = item.entry,
                        localSavePath = target.file,
                        targetType = target.type,
                        cloudPathSegments = cloudPath,
                        driveNameOverride = target.driveName,
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
            for (target in syncTargets) {
                val saveFile = File(target.path)
                if (!saveFile.exists()) continue
                val knownFileSiblingNames = fileSiblingCloudNames(target, mappingTargets, cloudFiles)
                val reservedSiblingNames = reservedCloudNames(target, mappingTargets, cloudFiles)
                try {
                    val result = postSync(
                        token = token,
                        entry = item.entry,
                        localSavePath = saveFile,
                        targetType = target.type,
                        cloudPathSegments = cloudPath,
                        knownFileSiblingNames = knownFileSiblingNames,
                        reservedSiblingNames = reservedSiblingNames,
                        cloudPath = target.cloudPath,
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
            if (e is SaveTargetMappingException) {
                onStatus(AppStatus.Error(e.message ?: "Invalid save mapping"))
                return false
            }
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
                        retryTargets = failedFileTargets,
                        completedBeforeRetry = completedCount,
                        blockedFailuresBeforeRetry = distinctBlockedReasons,
                        mappingTargets = mappingTargets,
                    )
                },
            )
        )
        return false
    }

    /** Establish a common save before launch, including explicit conflict resolutions. */
    private suspend fun preSync(
        token: String,
        entry: GameEntry,
        localSavePath: File,
        targetType: SaveTargetType,
        cloudPathSegments: List<String> = listOf(entry.effectiveCloudFolder),
        displayName: String = entry.name,
        reservedSiblingNames: Set<String> = emptySet(),
        cloudPath: String? = null,
    ) {
        val folderId = folders.ensurePath(token, cloudPathSegments)
        val cloudFiles = canonicalCloudFiles(search.listFilesInFolder(token, folderId))
        try {
            if (isDirectorySaveTarget(targetType, localSavePath, cloudFiles, reservedSiblingNames, cloudName = cloudPath)) {
                syncDirectoryManual(token, entry, localSavePath, folderId,
                    cloudFilesForSaveTarget(targetType, localSavePath,
                        projectDirectoryFiles(cloudFiles, cloudPath), reservedSiblingNames),
                    displayName, reservedSiblingNames, cloudPath, beforePlay = true)
            } else {
                syncFileManual(token, entry, localSavePath, folderId, cloudFiles, displayName, cloudPath ?: localSavePath.name, beforePlay = true)
            }
        } catch (failure: Exception) {
            if (failure is CancellationException || failure is SaveSyncException || failure is SaveTargetMappingException) throw failure
            throw SaveSyncException("Could not finish preparing saves for $displayName. Retry synchronization before playing.", failure)
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
        driveNameOverride: String? = null,
        knownFileSiblingNames: Set<String> = emptySet(),
        reservedSiblingNames: Set<String> = emptySet(),
        cloudPath: String? = null,
    ): PostSyncResult {
        if (!localSavePath.exists()) return PostSyncResult(0, emptyList())

        val driveName = driveNameOverride ?: cloudPath ?: localSavePath.name
        if (targetType == SaveTargetType.FILE && driveName in knownFileSiblingNames) {
            return PostSyncResult(
                0,
                emptyList(),
                listOf("The save name '$driveName' is already used by another configured file target."),
            )
        }

        val folderId = folders.ensurePath(token, cloudPathSegments)
        val rawCloudFiles = canonicalCloudFiles(search.listFilesInFolder(token, folderId))
        var completedCount = 0
        val failedTargets = mutableListOf<SaveUploadTarget>()
        val blockedReasons = mutableListOf<String>()

        val allReservedNames = knownFileSiblingNames + reservedSiblingNames

        // Legacy paths that did not exist during config migration remain UNKNOWN until
        // the game creates them. Resolve from the filesystem after the game exits.
        val isDirectory = isDirectorySaveTarget(targetType, localSavePath, rawCloudFiles, allReservedNames, cloudName = cloudPath)
        val cloudFiles = if (isDirectory) projectDirectoryFiles(rawCloudFiles, cloudPath) else rawCloudFiles
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
                        token, localFile, cloudFile, folderId, directoryCloudName(cloudPath, relativePath),
                        displayName = entry.name,
                    )
                    completedCount++
                } catch (e: Exception) {
                    if (e is CancellationException) throw e
                    failedTargets += SaveUploadTarget(
                        localFile, SaveTargetType.FILE, directoryCloudName(cloudPath, relativePath), e.message,
                        allReservedNames.mapTo(mutableSetOf()) { directoryCloudName(cloudPath, it) },
                    )
                }
            }
        } else {
            val cloudFile = cloudFiles.find { it.name == driveName }
            try {
                syncSingleFilePost(
                    token, localSavePath, cloudFile, folderId, driveName,
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
        displayName: String = driveName,
    ) {
        val decision = trackedSync.plan(localFile, cloudFile, parentFolderId, driveName)
        val choice = if (decision == SyncDecision.CONFLICT && cloudFile != null) {
            requestFileConflict(localFile, cloudFile, displayName)
        } else decision
        applyFileDecision(token, localFile, cloudFile, parentFolderId, driveName, choice)
    }

    companion object {
        private val dateFormatter = DateTimeFormatter
            .ofPattern("yyyy-MM-dd HH:mm:ss")
            .withZone(ZoneId.systemDefault())

        fun formatTimestamp(epochMs: Long): String =
            dateFormatter.format(Instant.ofEpochMilli(epochMs))
    }
}
