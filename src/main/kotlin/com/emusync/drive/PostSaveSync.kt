package com.emusync.drive

import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.SaveTargetType
import com.emusync.domain.library.effectiveCloudFolder
import com.emusync.domain.sync.SyncDecision
import java.io.File

internal class PostSaveSync(private val context: SyncContext, private val manual: ManualFileSync) {
    private val folders
        get() = context.folders

    private val search
        get() = context.search

    private val trackedSync
        get() = context.trackedSync

    suspend fun sync(
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
        val driveName = driveNameOverride ?: cloudPath ?: localSavePath.name
        blockedResult(localSavePath, targetType, driveName, knownFileSiblingNames)?.let {
            return it
        }

        val folderId = folders.ensurePath(token, cloudPathSegments)
        val rawCloudFiles = canonicalCloudFiles(search.listFilesInFolder(token, folderId))
        val allReservedNames = knownFileSiblingNames + reservedSiblingNames

        // Legacy paths that did not exist during config migration remain UNKNOWN until
        // the game creates them. Resolve from the filesystem after the game exits.
        val isDirectory =
            isDirectorySaveTarget(
                targetType,
                localSavePath,
                rawCloudFiles,
                allReservedNames,
                cloudName = cloudPath,
            )
        val cloudFiles =
            if (isDirectory) projectDirectoryFiles(rawCloudFiles, cloudPath) else rawCloudFiles
        return if (isDirectory) {
            uploadDirectory(
                DirectoryUpload(token, localSavePath, folderId, cloudFiles, entry.name, cloudPath),
                knownFileSiblingNames,
                reservedSiblingNames,
            )
        } else {
            uploadFile(
                token,
                PlannedSave(
                    localSavePath,
                    cloudFiles.find { it.name == driveName },
                    folderId,
                    driveName,
                ),
                entry.name,
                allReservedNames,
            )
        }
    }

    private fun blockedResult(
        local: File,
        type: SaveTargetType,
        name: String,
        siblings: Set<String>,
    ): PostSyncResult? =
        when {
            !local.exists() -> PostSyncResult(0, emptyList())
            type == SaveTargetType.FILE && name in siblings ->
                PostSyncResult(
                    0,
                    emptyList(),
                    listOf(
                        "The save name '$name' is already used by another configured file target."
                    ),
                )
            else -> null
        }

    private data class DirectoryUpload(
        val token: String,
        val root: File,
        val folderId: String,
        val cloudFiles: List<DriveFile>,
        val displayName: String,
        val prefix: String?,
    )

    private suspend fun uploadDirectory(
        directory: DirectoryUpload,
        siblings: Set<String>,
        reserved: Set<String>,
    ): PostSyncResult {
        var completed = 0
        val failed = mutableListOf<SaveUploadTarget>()
        val blocked = mutableListOf<String>()
        val allReserved =
            (siblings + reserved).mapTo(mutableSetOf()) { directoryCloudName(directory.prefix, it) }
        val localFiles = directory.root.walkTopDown().filter { it.isFile }.toList()
        for (file in localFiles) {
            val relative = file.toRelativeString(directory.root).replace(File.separatorChar, '/')
            when {
                relative in siblings ->
                    blocked +=
                        "The directory save '$relative' conflicts with another configured file target. Rename or reconfigure one of the saves before syncing it."
                relative !in reserved -> {
                    val planned =
                        PlannedSave(
                            file,
                            directory.cloudFiles.find { it.name == relative },
                            directory.folderId,
                            directoryCloudName(directory.prefix, relative),
                        )
                    val result =
                        uploadFile(directory.token, planned, directory.displayName, allReserved)
                    completed += result.completedCount
                    failed += result.failedTargets
                }
            }
        }
        return PostSyncResult(completed, failed, blocked)
    }

    private suspend fun uploadFile(
        token: String,
        file: PlannedSave,
        displayName: String,
        reserved: Set<String>,
    ): PostSyncResult =
        recoverDriveFailure(
            action = {
                syncSingleFilePost(token, file, displayName)
                PostSyncResult(1, emptyList())
            },
            recover = { failure ->
                PostSyncResult(
                    0,
                    listOf(
                        SaveUploadTarget(
                            file.local,
                            SaveTargetType.FILE,
                            file.driveName,
                            failure.message,
                            reserved,
                        )
                    ),
                )
            },
        )

    private suspend fun syncSingleFilePost(token: String, file: PlannedSave, displayName: String) {
        val decision = trackedSync.plan(file.local, file.remote, file.folderId, file.driveName)
        val choice =
            if (decision == SyncDecision.CONFLICT && file.remote != null) {
                manual.requestFileConflict(file.local, file.remote, displayName)
            } else decision
        manual.applyFileDecision(token, file, choice)
    }
}
