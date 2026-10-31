package com.emusync.drive

import com.emusync.domain.library.GameEntry
import com.emusync.domain.operation.recoverOperationFailure
import com.emusync.domain.sync.SyncEvent

internal class ManualEntrySync(
    private val context: SyncContext,
    private val paths: SaveTargetResolver,
    private val manual: ManualFileSync,
) {
    private val search
        get() = context.search

    private val folders
        get() = context.folders

    private val oauthFlow
        get() = context.oauthFlow

    private val config
        get() = context.config

    private val onStatus
        get() = context.onStatus

    /**
     * Manually synchronizes all saves for a given entry with Google Drive WITHOUT launching the
     * game or emulator.
     */
    suspend fun syncEntry(entry: GameEntry): Boolean {
        return recoverOperationFailure(
            action = {
                val token = connect() ?: return false

                val targets = paths.getSaveTargets(entry)
                syncTargets(entry, token, targets)
                onStatus(SyncEvent.Idle)
                true
            },
            recover = { e ->
                onStatus(SyncEvent.Error("Sync error: ${e.message}"))
                false
            },
        )
    }

    private suspend fun connect(): String? {
        if (config.cloudCredentials?.refreshToken.isNullOrBlank()) {
            onStatus(
                SyncEvent.Error(
                    "Google Drive is not connected. Click 'Connect Drive' in the top bar to sign in."
                )
            )
            return null
        }
        onStatus(SyncEvent.Syncing("Connecting to Google Drive..."))
        return recoverOperationFailure(
            action = { oauthFlow.authorize(config, allowInteractive = false) },
            recover = { failure ->
                onStatus(
                    SyncEvent.Error(
                        "Could not connect to Google Drive (${failure.message ?: "session expired"}). Please reconnect your account by clicking 'Connect Drive'."
                    )
                )
                null
            },
        )
    }

    private suspend fun syncTargets(
        entry: GameEntry,
        token: String,
        targets: List<ResolvedSaveTarget>,
    ) {
        val targetsByFolder = targets.groupBy { it.cloudPathSegments }
        for ((cloudPath, folderTargets) in targetsByFolder) {
            val folderId = folders.ensurePath(token, cloudPath)
            val cloudFiles = canonicalCloudFiles(search.listFilesInFolder(token, folderId))
            val configuredTargets = folderTargets.map { it.configured() }
            validateSaveTargetMappings(configuredTargets, cloudFiles)

            syncFolder(entry, token, folderTargets, folderId, cloudFiles)
        }
    }

    private suspend fun syncFolder(
        entry: GameEntry,
        token: String,
        folderTargets: List<ResolvedSaveTarget>,
        folderId: String,
        cloudFiles: List<DriveFile>,
    ) {
        val configuredTargets = folderTargets.map { it.configured() }
        for (target in folderTargets) {
            val savePath = target.file
            onStatus(SyncEvent.Syncing("Syncing ${target.displayName}..."))
            val reservedSiblingNames =
                reservedCloudNames(target.configured(), configuredTargets, cloudFiles)
            val isDirectory = target.configured().isDirectory(cloudFiles, configuredTargets)

            if (isDirectory) {
                val targetCloudFiles =
                    cloudFilesForSaveTarget(
                        target.type,
                        savePath,
                        projectDirectoryFiles(cloudFiles, target.cloudPath),
                        reservedSiblingNames,
                    )
                manual.syncDirectoryManual(
                    token,
                    entry,
                    savePath,
                    folderId,
                    targetCloudFiles,
                    displayName = target.displayName,
                    reservedSiblingNames = reservedSiblingNames,
                    cloudPrefix = target.cloudPath,
                )
            } else {
                manual.syncFileManual(
                    token,
                    entry,
                    savePath,
                    folderId,
                    cloudFiles,
                    displayName = target.displayName,
                    driveName = fileCloudName(target.configured()),
                )
            }
        }
    }
}
