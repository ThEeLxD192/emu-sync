package com.emusync.drive

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext

/** Moves cloud data without overwriting duplicate names or deleting recovery copies. */
internal class CloudFolderMigrator(
    private val folders: DriveFolders,
    private val search: DriveSearch,
) {
    suspend fun migrate(token: String, oldPath: List<String>, newPath: List<String>) {
        if (oldPath == newPath) return
        val sourceId = folders.findPath(token, oldPath) ?: return
        val destinationId = folders.findPath(token, newPath)
        if (sourceId == destinationId) return

        if (destinationId == null) {
            val oldParent = oldPath.dropLast(1)
            val newParent = newPath.dropLast(1)
            if (oldParent == newParent) {
                folders.renameFolder(token, sourceId, newPath.last())
            } else {
                folders.renameFolder(
                    token, sourceId, newPath.last(),
                    addParentId = folders.ensurePath(token, newParent),
                    removeParentId = folders.findPath(token, oldParent),
                )
            }
            return
        }

        val sourceFiles = search.listChildrenInFolder(token, sourceId)
        val destinationFiles = search.listChildrenInFolder(token, destinationId)
        val sourceDuplicates = sourceFiles.groupBy { it.name }.filterValues { it.size > 1 }.keys
        val destinationNames = destinationFiles.mapTo(mutableSetOf()) { it.name }
        val collisions = (sourceDuplicates + sourceFiles.map { it.name }.filter { it in destinationNames }).sorted()
        if (collisions.isNotEmpty()) {
            throw DriveApiException(
                "Cloud folders '${oldPath.joinToString("/")}' and '${newPath.joinToString("/")}' " +
                    "contain duplicate names: ${collisions.joinToString(", ")}. " +
                    "Both copies were kept. Resolve the duplicates in Google Drive before renaming.",
            )
        }

        val movedFiles = mutableListOf<DriveFile>()
        try {
            for (file in sourceFiles) {
                folders.moveFile(token, file.id, addParentId = destinationId, removeParentId = sourceId)
                movedFiles += file
            }
        } catch (failure: Exception) {
            val rollbackFailures = mutableListOf<String>()
            // A cancelled caller must still be able to restore acknowledged moves.
            withContext(NonCancellable) {
                for (file in movedFiles.asReversed()) {
                    try {
                        folders.moveFile(token, file.id, addParentId = sourceId, removeParentId = destinationId)
                    } catch (restoreFailure: Exception) {
                        rollbackFailures += file.name
                        failure.addSuppressed(restoreFailure)
                    }
                }
            }
            if (failure is CancellationException) throw failure
            val recovery = if (rollbackFailures.isEmpty()) {
                "Previously moved files were restored to '${oldPath.joinToString("/")}'."
            } else {
                "Some files remain in '${newPath.joinToString("/")}': ${rollbackFailures.joinToString(", ")}."
            }
            throw DriveApiException("Cloud migration failed: ${failure.message}. $recovery No folders were deleted.", failure)
        }

        // Keep the source folder: another device can add files after our listing,
        // and Drive has no atomic 'delete only if empty' operation here.
    }
}
