package com.emusync.drive

import com.emusync.model.GameEntry
import com.emusync.model.SaveTarget as ConfiguredSaveTarget
import com.emusync.model.SaveTargetType
import com.emusync.model.effectiveCloudFolder
import com.emusync.model.effectiveSaveTargets
import com.emusync.ui.AppStatus
import com.emusync.ui.GameItem
import java.io.File

/** Coordinates preparation and launch; retries and post-play uploads belong to PostGameSync. */
internal class GameSyncLifecycle(
    private val context: SyncContext,
    private val paths: SaveTargetResolver,
    private val manual: ManualFileSync,
) {
    private val postGame = PostGameSync(context, paths, manual)
    private val folders
        get() = context.folders

    private val search
        get() = context.search

    /**
     * Plays an item after any required restore, then uploads verified local progress.
     *
     * @param item The selected game.
     * @param resolveConfiguredItem Refreshes the item after save destinations are configured.
     * @param onRequestSaveSetup Requests destinations or returns an empty list when cancelled.
     */
    suspend fun playWithSync(
        item: GameItem,
        resolveConfiguredItem: (GameItem) -> GameItem = { it },
        onRequestSaveSetup: SaveSetupRequest = { _, _, _ -> emptyList() },
    ) {
        recoverDriveFailure(
            action = {
                val prepared =
                    PlayPreparation(
                        item,
                        item.effectiveSaveTargets,
                        paths.getCloudPathSegments(item),
                    )
                val hasDrive = context.config.googleDrive != null
                val ready =
                    !hasDrive || preparePlay(prepared, resolveConfiguredItem, onRequestSaveSetup)
                if (
                    ready &&
                        launchPrepared(
                            prepared,
                            hasDrive,
                            resolveConfiguredItem,
                            onRequestSaveSetup,
                        )
                )
                    context.onStatus(AppStatus.Idle)
            },
            recover = { context.onStatus(AppStatus.Error("Error: ${it.message}")) },
        )
    }

    private suspend fun preparePlay(
        prepared: PlayPreparation,
        resolve: (GameItem) -> GameItem,
        request: SaveSetupRequest,
    ): Boolean =
        recoverDriveFailure(
            action = {
                context.onStatus(AppStatus.Syncing("Checking cloud saves..."))
                val token =
                    context.oauthFlow.authorize(
                        context.config,
                        context.configManager,
                        allowInteractive = false,
                    )
                if (!ensureSaveSetup(token, prepared, resolve, request)) false
                else {
                    if (prepared.targets.isNotEmpty()) {
                        context.onStatus(
                            AppStatus.Syncing(
                                "Syncing saves for ${prepared.item.effectiveTitle}..."
                            )
                        )
                        val folderId = folders.ensurePath(token, prepared.cloudPath)
                        val cloudFiles =
                            canonicalCloudFiles(search.listFilesInFolder(token, folderId))
                        validateSaveTargetMappings(prepared.targets, cloudFiles)
                        for (target in prepared.targets) preSync(
                            token = token,
                            entry = prepared.item.entry,
                            localSavePath = File(target.path),
                            targetType = target.type,
                            cloudPathSegments = prepared.cloudPath,
                            displayName = prepared.item.effectiveTitle,
                            reservedSiblingNames =
                                reservedCloudNames(target, prepared.targets, cloudFiles),
                            cloudPath = target.cloudPath,
                        )
                    }
                    true
                }
            },
            recover = { failure ->
                if (failure is SaveTargetMappingException || failure is SaveSyncException)
                    throw failure
                true // Transport failures allow offline play; mapping and restore failures do not.
            },
        )

    private suspend fun ensureSaveSetup(
        token: String,
        prepared: PlayPreparation,
        resolve: (GameItem) -> GameItem,
        request: SaveSetupRequest,
    ): Boolean {
        if (prepared.targets.isNotEmpty()) return true
        val folderId = folders.findPath(token, prepared.cloudPath)
        val cloudFiles =
            if (folderId != null) search.listFilesInFolder(token, folderId) else emptyList()
        return if (cloudFiles.isEmpty()) true
        else {
            context.onStatus(AppStatus.Idle)
            val notice =
                "Cloud save data was found for this game. Choose one destination folder; only these listed files will be synchronized for this game."
            val requested = request(prepared.item, notice, cloudFiles.map { it.name })
            if (requested.isEmpty()) false
            else {
                prepared.targets = requested
                prepared.item = resolve(prepared.item)
                prepared.cloudPath = paths.getCloudPathSegments(prepared.item)
                true
            }
        }
    }

    private suspend fun launchPrepared(
        prepared: PlayPreparation,
        hasDrive: Boolean,
        resolve: (GameItem) -> GameItem,
        request: SaveSetupRequest,
    ): Boolean {
        context.onStatus(AppStatus.Playing)
        val entry = prepared.item.entry
        val result = context.runner.launch(entry, prepared.item.romFile, entry.steamAppId)
        if (result.exitCode != 0) {
            context.onStatus(
                AppStatus.Error(
                    "${prepared.item.effectiveTitle} did not start or its process could not be tracked. Cloud upload was not attempted."
                )
            )
            return false
        }
        if (prepared.targets.isEmpty()) {
            prepared.targets = request(prepared.item, null, emptyList())
            prepared.item = resolve(prepared.item)
        }
        return !hasDrive || postGame.uploadAfterPlay(prepared.item, prepared.targets)
    }

    private data class PlayPreparation(
        var item: GameItem,
        var targets: List<ConfiguredSaveTarget>,
        var cloudPath: List<String>,
    )

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
        recoverDriveFailure(
            action = {
                if (
                    isDirectorySaveTarget(
                        targetType,
                        localSavePath,
                        cloudFiles,
                        reservedSiblingNames,
                        cloudName = cloudPath,
                    )
                ) {
                    manual.syncDirectoryManual(
                        token,
                        entry,
                        localSavePath,
                        folderId,
                        cloudFilesForSaveTarget(
                            targetType,
                            localSavePath,
                            projectDirectoryFiles(cloudFiles, cloudPath),
                            reservedSiblingNames,
                        ),
                        displayName,
                        reservedSiblingNames,
                        cloudPath,
                        beforePlay = true,
                    )
                } else {
                    manual.syncFileManual(
                        token,
                        entry,
                        localSavePath,
                        folderId,
                        cloudFiles,
                        displayName,
                        cloudPath ?: localSavePath.name,
                        beforePlay = true,
                    )
                }
            },
            recover = { failure ->
                if (failure is SaveSyncException || failure is SaveTargetMappingException)
                    throw failure
                throw SaveSyncException(
                    "Could not finish preparing saves for $displayName. Retry synchronization before playing.",
                    failure,
                )
            },
        )
    }
}
