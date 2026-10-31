package com.emusync.application.sync

import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.SaveTarget as ConfiguredSaveTarget
import com.emusync.domain.operation.recoverOperationFailure
import com.emusync.domain.sync.CloudFile
import com.emusync.domain.sync.EntrySyncStatus
import com.emusync.domain.sync.SyncDecision
import com.emusync.domain.sync.canonicalCloudFiles

/** Reads local/remote observations without advancing synchronization history. */
internal class SyncStatusChecker(
    private val context: SyncContext,
    private val paths: SaveTargetResolver,
) {
    suspend fun checkEntrySyncStatus(entry: GameEntry): EntrySyncStatus {
        val drive = context.config.cloudCredentials
        if (drive == null || drive.refreshToken.isNullOrBlank())
            return EntrySyncStatus.NOT_CONFIGURED
        val targets = paths.getSaveTargets(entry)
        return if (targets.isEmpty()) EntrySyncStatus.IDLE else readStatus(targets)
    }

    private suspend fun readStatus(targets: List<ResolvedSaveTarget>): EntrySyncStatus =
        recoverOperationFailure(
            action = {
                val token =
                    context.oauthFlow.authorize(
                        context.config,
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
                    context.mapping.validateSaveTargetMappings(configured, cloudFiles)
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
            recover = { EntrySyncStatus.ERROR },
        )

    private suspend fun observeTarget(
        target: ResolvedSaveTarget,
        folderId: String,
        cloudFiles: List<CloudFile>,
        configured: List<ConfiguredSaveTarget>,
        observation: StatusObservation,
    ) {
        val reserved =
            context.mapping.reservedCloudNames(target.configured(), configured, cloudFiles)
        if (context.mapping.isDirectory(target.configured(), cloudFiles, configured)) {
            val remoteFiles =
                context.mapping.cloudFilesForSaveTarget(
                    target.type,
                    target.file,
                    projectDirectoryFiles(cloudFiles, target.cloudPath),
                    reserved,
                )
            observeDirectory(target, folderId, remoteFiles, reserved, observation)
        } else {
            val name = context.mapping.fileCloudName(target.configured())
            observation.record(
                context.trackedSync.plan(
                    (target.file).path,
                    (cloudFiles.find { it.name == name })?.toSyncState(),
                    folderId,
                    name,
                )
            )
        }
    }

    private suspend fun observeDirectory(
        target: ResolvedSaveTarget,
        folderId: String,
        remoteFiles: List<CloudFile>,
        reserved: Set<String>,
        observation: StatusObservation,
    ) {
        val root = target.file
        val localFiles =
            if (root.exists() && root.isDirectory) root.walkTopDown().filter { it.isFile }.toList()
            else emptyList()
        for (remote in remoteFiles) {
            val local = root.resolveRemote(remote.name) ?: continue
            observation.record(
                context.trackedSync.plan(
                    (local).path,
                    remote.toSyncState(),
                    folderId,
                    directoryCloudName(target.cloudPath, remote.name),
                )
            )
        }
        for (local in localFiles) {
            val relative = local.toRelativeString(root)
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

        fun status(): EntrySyncStatus =
            when {
                conflict || (localChanged && remoteChanged) -> EntrySyncStatus.CONFLICT
                localChanged || remoteChanged -> EntrySyncStatus.OUT_OF_SYNC
                else -> EntrySyncStatus.IN_SYNC
            }
    }
}
