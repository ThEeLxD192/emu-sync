package com.emusync.drive

import com.emusync.model.GameEntry
import com.emusync.model.SaveTarget as ConfiguredSaveTarget
import com.emusync.ui.CloudSyncStatus
import java.io.File

/** Reads local/remote observations without advancing synchronization history. */
internal class SyncStatusChecker(
    private val context: SyncContext,
    private val paths: SaveTargetResolver,
) {
    suspend fun checkEntrySyncStatus(entry: GameEntry): CloudSyncStatus {
        val drive = context.config.googleDrive
        if (drive == null || drive.refreshToken.isNullOrBlank())
            return CloudSyncStatus.NOT_CONFIGURED
        val targets = paths.getSaveTargets(entry)
        return if (targets.isEmpty()) CloudSyncStatus.IDLE else readStatus(targets)
    }

    private suspend fun readStatus(targets: List<SyncOrchestrator.SaveTarget>): CloudSyncStatus =
        recoverDriveFailure(
            action = {
                val token =
                    context.oauthFlow.authorize(
                        context.config,
                        context.configManager,
                        allowInteractive = false,
                    )
                val observation = StatusObservation()
                for ((path, folderTargets) in targets.groupBy { it.cloudPathSegments }) {
                    val folderId = context.folders.findPath(token, path)
                    val cloudFiles =
                        if (folderId != null)
                            canonicalCloudFiles(context.search.listFilesInFolder(token, folderId))
                        else emptyList()
                    val configured = folderTargets.map { it.configured() }
                    validateSaveTargetMappings(configured, cloudFiles)
                    for (target in folderTargets) observeTarget(
                        target,
                        folderId.orEmpty(),
                        cloudFiles,
                        configured,
                        observation,
                    )
                }
                observation.status()
            },
            recover = { CloudSyncStatus.ERROR },
        )

    private suspend fun observeTarget(
        target: SyncOrchestrator.SaveTarget,
        folderId: String,
        cloudFiles: List<DriveFile>,
        configured: List<ConfiguredSaveTarget>,
        observation: StatusObservation,
    ) {
        val reserved = reservedCloudNames(target.configured(), configured, cloudFiles)
        if (target.configured().isDirectory(cloudFiles, configured)) {
            val remoteFiles =
                cloudFilesForSaveTarget(
                    target.type,
                    target.file,
                    projectDirectoryFiles(cloudFiles, target.cloudPath),
                    reserved,
                )
            observeDirectory(target, folderId, remoteFiles, reserved, observation)
        } else {
            val name = fileCloudName(target.configured())
            observation.record(
                context.trackedSync.plan(
                    target.file,
                    cloudFiles.find { it.name == name },
                    folderId,
                    name,
                )
            )
        }
    }

    private suspend fun observeDirectory(
        target: SyncOrchestrator.SaveTarget,
        folderId: String,
        remoteFiles: List<DriveFile>,
        reserved: Set<String>,
        observation: StatusObservation,
    ) {
        val root = target.file
        val localFiles =
            if (root.exists() && root.isDirectory) root.walkTopDown().filter { it.isFile }.toList()
            else emptyList()
        for (remote in remoteFiles) {
            val local = resolveRemoteFile(root, remote.name) ?: continue
            observation.record(
                context.trackedSync.plan(
                    local,
                    remote,
                    folderId,
                    directoryCloudName(target.cloudPath, remote.name),
                )
            )
        }
        for (local in localFiles) {
            val relative = local.toRelativeString(root).replace(File.separatorChar, '/')
            if (relative !in reserved && remoteFiles.none { it.name == relative })
                observation.record(SyncDecision.UPLOAD_LOCAL)
        }
    }

    private class StatusObservation {
        private var localChanged = false
        private var remoteChanged = false
        private var conflict = false

        fun record(decision: SyncDecision) {
            when (decision) {
                SyncDecision.DOWNLOAD_CLOUD -> remoteChanged = true
                SyncDecision.UPLOAD_LOCAL -> localChanged = true
                SyncDecision.CONFLICT -> conflict = true
                SyncDecision.IN_SYNC -> Unit
            }
        }

        fun status(): CloudSyncStatus =
            when {
                conflict || (localChanged && remoteChanged) -> CloudSyncStatus.CONFLICT
                localChanged || remoteChanged -> CloudSyncStatus.OUT_OF_SYNC
                else -> CloudSyncStatus.IN_SYNC
            }
    }
}
