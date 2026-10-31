package com.emusync.drive

import com.emusync.domain.library.GameEntry
import com.emusync.domain.sync.SaveConflict
import com.emusync.domain.sync.SyncDecision
import com.emusync.local.saves.resolveRemoteFile
import java.io.File
import java.time.Instant

internal class ManualFileSync(private val context: SyncContext) {
    private val trackedSync
        get() = context.trackedSync

    suspend fun syncDirectoryManual(
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
        val actions =
            directoryActions(localSavePath, folderId, cloudFiles, reservedSiblingNames, cloudPrefix)
        val choice = directoryChoice(actions, displayName)
        for (action in actions) {
            val decision = directoryDecision(action, choice, beforePlay) ?: continue
            applyFileDecision(token, action.file, decision)
        }
    }

    private suspend fun directoryActions(
        root: File,
        folderId: String,
        cloudFiles: List<DriveFile>,
        reserved: Set<String>,
        prefix: String?,
    ): List<DirectoryAction> {
        val localFiles =
            if (root.isDirectory)
                root
                    .walkTopDown()
                    .filter {
                        it.isFile &&
                            it.toRelativeString(root).replace(File.separatorChar, '/') !in reserved
                    }
                    .associateBy { it.toRelativeString(root).replace(File.separatorChar, '/') }
            else emptyMap()
        val remoteFiles =
            canonicalCloudFiles(cloudFiles)
                .filter { resolveRemoteFile(root, it.name) != null }
                .associateBy { it.name }
        return (localFiles.keys + remoteFiles.keys).sorted().map { name ->
            val local =
                localFiles[name]
                    ?: checkNotNull(resolveRemoteFile(root, name)) {
                        "Invalid remote save path: $name"
                    }
            val remote = remoteFiles[name]
            val driveName = directoryCloudName(prefix, name)
            DirectoryAction(
                PlannedSave(local, remote, folderId, driveName),
                trackedSync.plan((local).path, (remote)?.toSyncState(), folderId, driveName),
            )
        }
    }

    private suspend fun directoryChoice(
        actions: List<DirectoryAction>,
        displayName: String,
    ): SyncDecision? {
        val conflict = actions.firstOrNull { it.decision == SyncDecision.CONFLICT }
        val mixed =
            actions.any { it.decision == SyncDecision.UPLOAD_LOCAL } &&
                actions.any { it.decision == SyncDecision.DOWNLOAD_CLOUD }
        return if (conflict != null || mixed) {
            val representative = conflict ?: actions.first { it.file.remote != null }
            requestFileConflict(
                representative.file.local,
                checkNotNull(representative.file.remote) { "A conflict requires a remote file" },
                displayName,
            )
        } else null
    }

    private fun directoryDecision(
        action: DirectoryAction,
        choice: SyncDecision?,
        beforePlay: Boolean,
    ): SyncDecision? {
        val decision =
            when {
                action.decision == SyncDecision.IN_SYNC -> SyncDecision.IN_SYNC
                choice == SyncDecision.DOWNLOAD_CLOUD ->
                    if (action.file.remote != null) choice else null
                choice == SyncDecision.UPLOAD_LOCAL ->
                    if (action.file.local.isFile) choice else null
                else -> action.decision
            }
        val deferredUpload = beforePlay && choice == null && decision == SyncDecision.UPLOAD_LOCAL
        return decision?.takeUnless { deferredUpload }
    }

    private data class DirectoryAction(val file: PlannedSave, val decision: SyncDecision)

    suspend fun syncFileManual(
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
        val decision =
            trackedSync.plan((localSavePath).path, (remote)?.toSyncState(), folderId, driveName)
        val choice =
            if (decision == SyncDecision.CONFLICT && remote != null) {
                requestFileConflict(localSavePath, remote, displayName)
            } else decision
        if (!(beforePlay && decision == SyncDecision.UPLOAD_LOCAL)) {
            applyFileDecision(
                token,
                PlannedSave(localSavePath, remote, folderId, driveName),
                choice,
            )
        }
    }

    suspend fun requestFileConflict(
        local: File,
        remote: DriveFile,
        displayName: String,
    ): SyncDecision =
        context.requestConflict(
            SaveConflict(
                localModifiedTimeMs = local.lastModified(),
                cloudModifiedTimeMs = Instant.parse(remote.modifiedTime).toEpochMilli(),
                gameName = displayName,
            )
        )

    suspend fun applyFileDecision(token: String, file: PlannedSave, decision: SyncDecision?) {
        val local = file.local
        val remote = file.remote
        val folderId = file.folderId
        val driveName = file.driveName

        when (decision) {
            SyncDecision.DOWNLOAD_CLOUD ->
                remote?.let {
                    trackedSync.download(
                        token,
                        (it).toSyncState(),
                        (local).path,
                        folderId,
                        driveName,
                    )
                }
            SyncDecision.UPLOAD_LOCAL ->
                if (local.isFile)
                    trackedSync.upload(
                        token,
                        (local).path,
                        (remote)?.toSyncState(),
                        folderId,
                        driveName,
                    )
            SyncDecision.IN_SYNC ->
                if (local.isFile && remote != null)
                    trackedSync.remember((local).path, (remote).toSyncState(), folderId, driveName)
            null -> Unit
            SyncDecision.CONFLICT -> error("A conflict must be resolved before applying a save")
        }
    }
}
