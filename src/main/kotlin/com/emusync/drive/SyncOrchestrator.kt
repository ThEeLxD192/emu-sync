package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.model.AppConfig
import com.emusync.model.GameEntry
import com.emusync.model.EmulatorSystem
import com.emusync.model.NativePCGame
import com.emusync.runner.GameRunner
import com.emusync.ui.AppStatus
import com.emusync.ui.GameItem
import io.ktor.client.*
import java.io.File
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter

/**
 * Orchestrates the full game lifecycle:
 *
 * 1. **Pre-Sync**: Download the latest save from Drive (if newer)
 * 2. **Play**: Launch the game via [GameRunner]
 * 3. **Post-Sync**: Upload the local save to Drive (if modified during play)
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
    private val auth = DriveAuth(client)
    private val search = DriveSearch(client)
    private val transfer = DriveTransfer(client)
    private val folders = DriveFolders(client)
    private val oauthFlow = OAuthFlow(client)
    private val runner = GameRunner()

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
            // Cache the access token so we don't trigger a second browser auth.
            // The `config` field is a snapshot from construction time; after
            // authorize() saves a new refresh token via configManager, this
            // snapshot still has the old/null value. Re-reading the fresh config
            // for the post-sync call avoids the duplicate prompt.
            var cachedToken: String? = null

            // ── Step 1: Pre-Sync (download cloud save if newer) ─────
            if (hasDriveConfig) {
                try {
                    onStatus(AppStatus.Syncing("Checking cloud saves..."))
                    cachedToken = oauthFlow.authorize(config, configManager, allowInteractive = false)

                    onStatus(AppStatus.Syncing("Syncing saves for ${item.entry.name}..."))
                    for (pathStr in item.effectiveSavePaths) {
                        val saveFile = File(pathStr)
                        preSync(cachedToken, item.entry, saveFile)
                    }
                } catch (_: Exception) {
                    // Offline mode, network timeout, or not logged in:
                    // Proceed immediately to play with local saves without blocking or delaying!
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
            if (currentSyncPaths.isEmpty() && result.exitCode == 0) { // Only ask if the emulator actually ran successfully
                currentSyncPaths = onRequestSaveSetup(item)
            }

            // ── Step 3: Post-Sync (upload local save if modified) ───
            if (hasDriveConfig && cachedToken != null) {
                try {
                    onStatus(AppStatus.Syncing("Uploading save for ${item.entry.name}..."))
                    for (pathStr in currentSyncPaths) {
                        val saveFile = File(pathStr)
                        if (saveFile.exists()) {
                            postSync(cachedToken, item.entry, saveFile)
                        }
                    }
                } catch (_: Exception) {
                    // Offline or upload failed: local save is kept safely on disk
                }
            }

            onStatus(AppStatus.Idle)

        } catch (e: Exception) {
            onStatus(AppStatus.Error("Error: ${e.message}"))
        }
    }

    /**
     * Pre-Sync: Check cloud save timestamp vs local. Download if cloud is newer.
     */
    private suspend fun preSync(token: String, entry: GameEntry, localSavePath: File) {
        val folderId = folders.ensureEntryFolder(token, entry.name)
        val cloudFiles = search.listFilesInFolder(token, folderId)

        // Determine if localSavePath is meant to be a directory
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
                        gameName = localSavePath.name,
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
                // If UPLOAD_LOCAL: keep local files as-is. Post-sync will upload them to Drive after the game exits.
            } else {
                // No conflict: download files where cloud is newer or missing locally
                for (pair in pairs) {
                    if (pair.decision == SyncDecision.DOWNLOAD_CLOUD) {
                        pair.localFile.parentFile?.mkdirs()
                        transfer.download(token, pair.cloudFile.id, pair.localFile)
                    }
                }
            }
        } else {
            val cloudFile = cloudFiles.find { it.name == localSavePath.name } ?: return

            if (!localSavePath.exists()) {
                localSavePath.parentFile?.mkdirs()
                transfer.download(token, cloudFile.id, localSavePath)
                return
            }

            val decision = resolveConflict(
                localModifiedTimeMs = localSavePath.lastModified(),
                cloudModifiedTimeIso = cloudFile.modifiedTime,
            )

            when (decision) {
                SyncDecision.DOWNLOAD_CLOUD -> {
                    transfer.download(token, cloudFile.id, localSavePath)
                }
                SyncDecision.UPLOAD_LOCAL, SyncDecision.IN_SYNC -> {
                    // Nothing to download in pre-sync
                }
                SyncDecision.CONFLICT -> {
                    val localDate = formatTimestamp(localSavePath.lastModified())
                    val cloudDate = formatTimestamp(Instant.parse(cloudFile.modifiedTime).toEpochMilli())

                    val choice: SyncDecision = kotlinx.coroutines.suspendCancellableCoroutine { continuation ->
                        onStatus(AppStatus.Conflict(
                            localDate = localDate,
                            cloudDate = cloudDate,
                            gameName = localSavePath.name,
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
                }
            }
        }
    }

    /**
     * Post-Sync: Upload local save to the entry's subfolder on Drive.
     */
    private suspend fun postSync(token: String, entry: GameEntry, localSavePath: File) {
        if (!localSavePath.exists()) return

        val folderId = folders.ensureEntryFolder(token, entry.name)
        val cloudFiles = search.listFilesInFolder(token, folderId)

        if (localSavePath.isDirectory) {
            val localFiles = localSavePath.walkTopDown().filter { it.isFile }.toList()
            for (localFile in localFiles) {
                val relativePath = localFile.toRelativeString(localSavePath).replace(File.separatorChar, '/')
                val cloudFile = cloudFiles.find { it.name == relativePath }
                syncSingleFilePost(token, localFile, cloudFile, folderId, relativePath)
            }
        } else {
            val cloudFile = cloudFiles.find { it.name == localSavePath.name }
            syncSingleFilePost(token, localSavePath, cloudFile, folderId, localSavePath.name)
        }
    }

    private suspend fun syncSingleFilePost(token: String, localFile: File, cloudFile: DriveFile?, parentFolderId: String, driveName: String) {
        if (cloudFile != null) {
            // Only upload if the local file is strictly newer (or modified during play)
            val cloudMs = Instant.parse(cloudFile.modifiedTime).toEpochMilli()
            val isNewerLocally = localFile.lastModified() > cloudMs
            
            if (isNewerLocally) {
                transfer.update(token, cloudFile.id, localFile)
            }
        } else {
            // Upload as new file with full relative path string preserving structure
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
