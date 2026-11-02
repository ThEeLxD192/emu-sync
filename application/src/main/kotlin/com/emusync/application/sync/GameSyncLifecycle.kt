package com.emusync.application.sync

import com.emusync.domain.execution.GameLaunchRequest
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.SaveTarget as ConfiguredSaveTarget
import com.emusync.domain.library.SaveTargetType
import com.emusync.domain.library.effectiveCloudFolder
import com.emusync.domain.operation.recoverOperationFailure
import com.emusync.domain.sync.LocalSavePath
import com.emusync.domain.sync.SaveSetup
import com.emusync.domain.sync.SaveSetupRequest
import com.emusync.domain.sync.SaveSyncException
import com.emusync.domain.sync.SyncEvent
import com.emusync.domain.sync.SyncGame
import com.emusync.domain.sync.canonicalCloudFiles
import com.emusync.domain.sync.cloudPathSegments

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

    suspend fun playWithSync(game: SyncGame, setup: SaveSetup) {
        recoverOperationFailure(
            action = {
                val prepared =
                    PlayPreparation(
                        game,
                        game.effectiveSaveTargets,
                        paths.getCloudPathSegments(game),
                    )
                val hasDrive = context.config.cloudCredentials != null
                val ready = !hasDrive || preparePlay(prepared, setup)
                if (ready && launchPrepared(prepared, hasDrive, setup))
                    context.onStatus(SyncEvent.Idle)
            },
            recover = { context.onStatus(SyncEvent.Error("Error: ${it.message}")) },
        )
    }

    private suspend fun preparePlay(prepared: PlayPreparation, setup: SaveSetup): Boolean =
        recoverOperationFailure(
            action = {
                context.onStatus(SyncEvent.Syncing("Checking cloud saves..."))
                val token =
                    context.oauthFlow.authorize(
                        context.config,
                        allowInteractive = false,
                    )
                if (!ensureSaveSetup(token, prepared, setup)) false
                else {
                    if (prepared.targets.isNotEmpty()) {
                        context.onStatus(
                            SyncEvent.Syncing(
                                "Syncing saves for ${prepared.item.effectiveTitle}..."
                            )
                        )
                        val folderId = folders.ensurePath(token, prepared.cloudPath)
                        val cloudFiles =
                            canonicalCloudFiles(search.listFilesInFolder(token, folderId))
                        context.mapping.validateSaveTargetMappings(prepared.targets, cloudFiles)
                        for (target in prepared.targets) preSync(
                            token = token,
                            entry = prepared.item.entry,
                            localSavePath = context.files.path(target.path),
                            targetType = target.type,
                            cloudPathSegments = prepared.cloudPath,
                            displayName = prepared.item.effectiveTitle,
                            reservedSiblingNames =
                                context.mapping.reservedCloudNames(
                                    target,
                                    prepared.targets,
                                    cloudFiles,
                                ),
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
        setup: SaveSetup,
    ): Boolean {
        if (prepared.targets.isNotEmpty()) return true
        val folderId = folders.findPath(token, prepared.cloudPath)
        val cloudFiles =
            if (folderId != null) search.listFilesInFolder(token, folderId) else emptyList()
        return if (cloudFiles.isEmpty()) true
        else {
            context.onStatus(SyncEvent.Idle)
            val requested =
                setup.request(SaveSetupRequest(prepared.item, cloudFiles.map { it.name }))
            if (requested.targets.isEmpty()) false
            else {
                prepared.targets = requested.targets
                prepared.item = requested.game
                prepared.cloudPath = paths.getCloudPathSegments(prepared.item)
                true
            }
        }
    }

    private suspend fun launchPrepared(
        prepared: PlayPreparation,
        hasDrive: Boolean,
        setup: SaveSetup,
    ): Boolean {
        context.onStatus(SyncEvent.Playing)
        val entry = prepared.item.entry
        val result =
            context.runner.launch(GameLaunchRequest(entry, prepared.item.romPath, entry.steamAppId))
        if (result.exitCode != 0) {
            context.onStatus(
                SyncEvent.Error(
                    "${prepared.item.effectiveTitle} did not start or its process could not be tracked. Cloud upload was not attempted."
                )
            )
            return false
        }
        if (prepared.targets.isEmpty()) {
            val requested = setup.request(SaveSetupRequest(prepared.item))
            prepared.targets = requested.targets
            prepared.item = requested.game
        }
        return !hasDrive || postGame.uploadAfterPlay(prepared.item, prepared.targets)
    }

    private data class PlayPreparation(
        var item: SyncGame,
        var targets: List<ConfiguredSaveTarget>,
        var cloudPath: List<String>,
    )

    private suspend fun preSync(
        token: String,
        entry: GameEntry,
        localSavePath: LocalSavePath,
        targetType: SaveTargetType,
        cloudPathSegments: List<String> = listOf(entry.effectiveCloudFolder),
        displayName: String = entry.name,
        reservedSiblingNames: Set<String> = emptySet(),
        cloudPath: String? = null,
    ) {
        val folderId = folders.ensurePath(token, cloudPathSegments)
        val cloudFiles = canonicalCloudFiles(search.listFilesInFolder(token, folderId))
        recoverOperationFailure(
            action = {
                if (
                    context.mapping.isDirectorySaveTarget(
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
                        context.mapping.cloudFilesForSaveTarget(
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
