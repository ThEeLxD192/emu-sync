package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.model.AppConfig
import com.emusync.model.GameEntry
import com.emusync.model.EmulatorSystem
import com.emusync.model.NativePCGame
import com.emusync.runner.GameRunner
import com.emusync.ui.AppStatus
import com.emusync.ui.CloudSyncStatus
import com.emusync.ui.GameItem
import io.ktor.client.*
import java.io.File
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter

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
    private val search = DriveSearch(client)
    private val transfer = DriveTransfer(client)
    private val folders = DriveFolders(client)
    private val oauthFlow = OAuthFlow(client)
    private val runner = GameRunner()

    /**
     * Helper to retrieve all configured save paths for an entry.
     */
    fun getAllSavePaths(entry: GameEntry): List<File> {
        val paths = when (entry) {
            is NativePCGame -> entry.savePaths
            is EmulatorSystem -> (entry.savePaths + entry.savePathsByRom.values.flatten()).distinct()
        }
        return paths.filter { it.isNotBlank() }.map { File(it) }
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

        val savePaths = getAllSavePaths(entry)
        if (savePaths.isEmpty()) {
            return CloudSyncStatus.IDLE
        }

        return try {
            val token = oauthFlow.authorize(config, configManager, allowInteractive = false)
            val folderId = folders.ensureEntryFolder(token, entry.name)
            val cloudFiles = search.listFilesInFolder(token, folderId)

            var hasNewerLocal = false
            var hasNewerCloud = false
            var hasConflict = false
            var anyFileChecked = false

            for (savePath in savePaths) {
                val isDirectory = when {
                    savePath.exists() -> savePath.isDirectory
                    savePath.extension.isNotEmpty() -> false
                    cloudFiles.isEmpty() -> false
                    cloudFiles.size == 1 && cloudFiles[0].name == savePath.name -> false
                    else -> true
                }

                if (isDirectory) {
                    val localFiles = if (savePath.exists() && savePath.isDirectory) {
                        savePath.walkTopDown().filter { it.isFile }.toList()
                    } else {
                        emptyList()
                    }

                    if (localFiles.isEmpty() && cloudFiles.isEmpty()) continue
                    anyFileChecked = true

                    for (cloudFile in cloudFiles) {
                        val relPath = cloudFile.name.replace('/', File.separatorChar)
                        val localFile = File(savePath, relPath)
                        if (!localFile.exists()) {
                            hasNewerCloud = true
                        } else {
                            val decision = resolveConflict(localFile.lastModified(), cloudFile.modifiedTime)
                            when (decision) {
                                SyncDecision.DOWNLOAD_CLOUD -> hasNewerCloud = true
                                SyncDecision.CONFLICT -> hasConflict = true
                                SyncDecision.UPLOAD_LOCAL -> hasNewerLocal = true
                                SyncDecision.IN_SYNC -> {}
                            }
                        }
                    }

                    for (localFile in localFiles) {
                        val relPath = localFile.toRelativeString(savePath).replace(File.separatorChar, '/')
                        if (cloudFiles.none { it.name == relPath }) {
                            hasNewerLocal = true
                        }
                    }
                } else {
                    val cloudFile = cloudFiles.find { it.name == savePath.name }
                    val localExists = savePath.exists()
                    val cloudExists = cloudFile != null

                    if (!localExists && !cloudExists) continue
                    anyFileChecked = true

                    if (!localExists && cloudExists) {
                        hasNewerCloud = true
                    } else if (localExists && !cloudExists) {
                        hasNewerLocal = true
                    } else if (localExists && cloudFile != null) {
                        val decision = resolveConflict(savePath.lastModified(), cloudFile.modifiedTime)
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
                !anyFileChecked -> CloudSyncStatus.IDLE
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
            val driveConfig = config.googleDrive ?: return false
            if (driveConfig.refreshToken.isNullOrBlank()) return false

            onStatus(AppStatus.Syncing("Conectando con Google Drive..."))
            val token = oauthFlow.authorize(config, configManager, allowInteractive = false)

            val savePaths = getAllSavePaths(entry)
            if (savePaths.isEmpty()) {
                onStatus(AppStatus.Idle)
                return true
            }

            val folderId = folders.ensureEntryFolder(token, entry.name)
            val cloudFiles = search.listFilesInFolder(token, folderId)

            for (savePath in savePaths) {
                onStatus(AppStatus.Syncing("Sincronizando ${entry.name}..."))
                val isDirectory = when {
                    savePath.exists() -> savePath.isDirectory
                    savePath.extension.isNotEmpty() -> false
                    cloudFiles.isEmpty() -> false
                    cloudFiles.size == 1 && cloudFiles[0].name == savePath.name -> false
                    else -> true
                }

                if (isDirectory) {
                    syncDirectoryManual(token, entry, savePath, folderId, cloudFiles)
                } else {
                    syncFileManual(token, entry, savePath, folderId, cloudFiles)
                }
            }

            onStatus(AppStatus.Idle)
            true
        } catch (e: Exception) {
            onStatus(AppStatus.Error("Error al sincronizar: ${e.message}"))
            false
        }
    }

    private suspend fun syncDirectoryManual(
        token: String,
        entry: GameEntry,
        localSavePath: File,
        folderId: String,
        cloudFiles: List<DriveFile>,
    ) {
        data class FilePair(val localFile: File, val cloudFile: DriveFile, val decision: SyncDecision)

        val pairs = cloudFiles.map { cloudFile ->
            val relativePath = cloudFile.name.replace('/', File.separatorChar)
            val localFile = File(localSavePath, relativePath)
            val decision = if (!localFile.exists()) {
                SyncDecision.DOWNLOAD_CLOUD
            } else {
                resolveConflict(localFile.lastModified(), cloudFile.modifiedTime)
            }
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
            cloudFiles.none { it.name == rel }
        }
        val hasCloudNewer = pairs.any { it.decision == SyncDecision.DOWNLOAD_CLOUD }

        if (conflictingPairs.isNotEmpty() || (hasLocalOnly && hasCloudNewer)) {
            val latestLocalMs = if (localFiles.isNotEmpty()) localFiles.maxOf { it.lastModified() } else System.currentTimeMillis()
            val latestCloudMs = if (cloudFiles.isNotEmpty()) cloudFiles.maxOf { Instant.parse(it.modifiedTime).toEpochMilli() } else System.currentTimeMillis()

            val choice: SyncDecision = kotlinx.coroutines.suspendCancellableCoroutine { continuation ->
                onStatus(AppStatus.Conflict(
                    localDate = formatTimestamp(latestLocalMs),
                    cloudDate = formatTimestamp(latestCloudMs),
                    gameName = entry.name,
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
                    transfer.download(token, pair.cloudFile.id, pair.localFile)
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
                    transfer.download(token, pair.cloudFile.id, pair.localFile)
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
    ) {
        val cloudFile = cloudFiles.find { it.name == localSavePath.name }

        if (!localSavePath.exists() && cloudFile != null) {
            localSavePath.parentFile?.mkdirs()
            transfer.download(token, cloudFile.id, localSavePath)
            return
        }

        if (localSavePath.exists() && cloudFile == null) {
            transfer.upload(token, localSavePath, driveName = localSavePath.name, parentFolderId = folderId)
            return
        }

        if (localSavePath.exists() && cloudFile != null) {
            val decision = resolveConflict(localSavePath.lastModified(), cloudFile.modifiedTime)
            when (decision) {
                SyncDecision.DOWNLOAD_CLOUD -> {
                    transfer.download(token, cloudFile.id, localSavePath)
                }
                SyncDecision.UPLOAD_LOCAL -> {
                    transfer.update(token, cloudFile.id, localSavePath)
                }
                SyncDecision.IN_SYNC -> {
                    // Up to date
                }
                SyncDecision.CONFLICT -> {
                    val localDate = formatTimestamp(localSavePath.lastModified())
                    val cloudDate = formatTimestamp(Instant.parse(cloudFile.modifiedTime).toEpochMilli())

                    val choice: SyncDecision = kotlinx.coroutines.suspendCancellableCoroutine { continuation ->
                        onStatus(AppStatus.Conflict(
                            localDate = localDate,
                            cloudDate = cloudDate,
                            gameName = entry.name,
                            onResolve = { userChoice ->
                                if (continuation.isActive) {
                                    continuation.resumeWith(Result.success(userChoice))
                                }
                            }
                        ))
                    }

                    if (choice == SyncDecision.DOWNLOAD_CLOUD) {
                        transfer.download(token, cloudFile.id, localSavePath)
                    } else if (choice == SyncDecision.UPLOAD_LOCAL) {
                        transfer.update(token, cloudFile.id, localSavePath)
                    }
                }
            }
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
        onRequestSaveSetup: suspend (GameItem) -> List<String> = { emptyList() }
    ) {
        try {
            val hasDriveConfig = config.googleDrive != null
            var cachedToken: String? = null
            var forceUploadLocal = false

            // ── Step 1: Pre-Sync (download cloud save if newer) ─────
            if (hasDriveConfig) {
                try {
                    onStatus(AppStatus.Syncing("Checking cloud saves..."))
                    cachedToken = oauthFlow.authorize(config, configManager, allowInteractive = false)

                    onStatus(AppStatus.Syncing("Syncing saves for ${item.name}..."))
                    for (pathStr in item.effectiveSavePaths) {
                        val saveFile = File(pathStr)
                        val decision = preSync(cachedToken, item.entry, saveFile, displayName = item.name)
                        if (decision == SyncDecision.UPLOAD_LOCAL) {
                            forceUploadLocal = true
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
            var currentSyncPaths = item.effectiveSavePaths
            if (currentSyncPaths.isEmpty() && result.exitCode == 0) {
                currentSyncPaths = onRequestSaveSetup(item)
            }

            // ── Step 3: Post-Sync (upload local save if modified) ───
            if (hasDriveConfig && cachedToken != null) {
                try {
                    onStatus(AppStatus.Syncing("Uploading save for ${item.name}..."))
                    for (pathStr in currentSyncPaths) {
                        val saveFile = File(pathStr)
                        if (saveFile.exists()) {
                            postSync(cachedToken, item.entry, saveFile, forceAll = forceUploadLocal)
                        }
                    }
                } catch (_: Exception) {
                }
            }

            onStatus(AppStatus.Idle)

        } catch (e: Exception) {
            onStatus(AppStatus.Error("Error: ${e.message}"))
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
        displayName: String = entry.name
    ): SyncDecision? {
        val folderId = folders.ensureEntryFolder(token, entry.name)
        val cloudFiles = search.listFilesInFolder(token, folderId)

        val isDirectory = when {
            localSavePath.exists() -> localSavePath.isDirectory
            localSavePath.extension.isNotEmpty() -> false
            cloudFiles.isEmpty() -> false
            cloudFiles.size == 1 && cloudFiles[0].name == localSavePath.name -> false
            else -> true
        }

        if (isDirectory) {
            data class FilePair(val localFile: File, val cloudFile: DriveFile, val decision: SyncDecision)

            val pairs = cloudFiles.map { cloudFile ->
                val relativePath = cloudFile.name.replace('/', File.separatorChar)
                val localFile = File(localSavePath, relativePath)
                val decision = if (!localFile.exists()) {
                    SyncDecision.DOWNLOAD_CLOUD
                } else {
                    resolveConflict(
                        localModifiedTimeMs = localFile.lastModified(),
                        cloudModifiedTimeIso = cloudFile.modifiedTime,
                    )
                }
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
                        transfer.download(token, pair.cloudFile.id, pair.localFile)
                    }
                }
                return choice
            } else {
                for (pair in pairs) {
                    if (pair.decision == SyncDecision.DOWNLOAD_CLOUD) {
                        pair.localFile.parentFile?.mkdirs()
                        transfer.download(token, pair.cloudFile.id, pair.localFile)
                    }
                }
                return null
            }
        } else {
            val cloudFile = cloudFiles.find { it.name == localSavePath.name } ?: return null

            if (!localSavePath.exists()) {
                localSavePath.parentFile?.mkdirs()
                transfer.download(token, cloudFile.id, localSavePath)
                return null
            }

            val decision = resolveConflict(
                localModifiedTimeMs = localSavePath.lastModified(),
                cloudModifiedTimeIso = cloudFile.modifiedTime,
            )

            when (decision) {
                SyncDecision.DOWNLOAD_CLOUD -> {
                    transfer.download(token, cloudFile.id, localSavePath)
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
                        transfer.download(token, cloudFile.id, localSavePath)
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
        forceAll: Boolean = false
    ) {
        if (!localSavePath.exists()) return

        val folderId = folders.ensureEntryFolder(token, entry.name)
        val cloudFiles = search.listFilesInFolder(token, folderId)

        if (localSavePath.isDirectory) {
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
            val cloudMs = Instant.parse(cloudFile.modifiedTime).toEpochMilli()
            val isNewerLocally = localFile.lastModified() > cloudMs

            if (force || isNewerLocally) {
                transfer.update(token, cloudFile.id, localFile)
            }
        } else {
            transfer.upload(token, localFile, driveName = driveName, parentFolderId = parentFolderId)
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
