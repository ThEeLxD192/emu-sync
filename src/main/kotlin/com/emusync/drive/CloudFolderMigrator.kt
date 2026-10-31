package com.emusync.drive

import com.emusync.domain.sync.CloudFolderStore
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext

/** Moves cloud data without overwriting duplicate names or deleting recovery copies. */
internal class CloudFolderMigrator(
    private val folders: CloudFolderStore,
    private val search: DriveSearch,
) {
    suspend fun migrate(token: String, oldPath: List<String>, newPath: List<String>) {
        if (oldPath != newPath) {
            folders.findPath(token, oldPath)?.let { source ->
                val destination = folders.findPath(token, newPath)
                if (source != destination) {
                    if (destination == null) relocate(token, source, oldPath, newPath)
                    else merge(token, CloudMove(source, destination, oldPath, newPath))
                }
            }
        }
    }

    private suspend fun relocate(
        token: String,
        source: String,
        oldPath: List<String>,
        newPath: List<String>,
    ) {
        val oldParent = oldPath.dropLast(1)
        val newParent = newPath.dropLast(1)
        if (oldParent == newParent) folders.renameFolder(token, source, newPath.last())
        else
            folders.renameFolder(
                token,
                source,
                newPath.last(),
                addParentId = folders.ensurePath(token, newParent),
                removeParentId = folders.findPath(token, oldParent),
            )
    }

    private suspend fun merge(token: String, move: CloudMove) {
        val sourceFiles = search.listChildrenInFolder(token, move.source)
        val destinationFiles = search.listChildrenInFolder(token, move.destination)
        val duplicates = sourceFiles.groupBy { it.name }.filterValues { it.size > 1 }.keys
        val destinationNames = destinationFiles.mapTo(mutableSetOf()) { it.name }
        val collisions =
            (duplicates + sourceFiles.map { it.name }.filter { it in destinationNames }).sorted()
        if (collisions.isNotEmpty())
            throw DriveApiException(
                "Cloud folders '${move.oldPath.joinToString("/")}' and '${move.newPath.joinToString("/")}' " +
                    "contain duplicate names: ${collisions.joinToString(", ")}. " +
                    "Both copies were kept. Resolve the duplicates in Google Drive before renaming."
            )
        moveWithRollback(token, move, sourceFiles)
        // Retain the source folder: another device can add files after the listing.
    }

    // Restore acknowledged moves in NonCancellable before propagating cancellation or failure.
    @Suppress("TooGenericExceptionCaught", "SuspendFunSwallowedCancellation")
    private suspend fun moveWithRollback(
        token: String,
        move: CloudMove,
        sourceFiles: List<DriveFile>,
    ) {
        val moved = mutableListOf<DriveFile>()
        try {
            for (file in sourceFiles) {
                folders.moveFile(
                    token,
                    file.id,
                    addParentId = move.destination,
                    removeParentId = move.source,
                )
                moved += file
            }
        } catch (failure: Exception) {
            val rollbackFailures = rollbackMoves(token, move, moved, failure)
            if (failure is CancellationException) throw failure
            val recovery =
                if (rollbackFailures.isEmpty()) {
                    "Previously moved files were restored to '${move.oldPath.joinToString("/")}'."
                } else {
                    "Some files remain in '${move.newPath.joinToString("/")}': ${rollbackFailures.joinToString(", ")}."
                }
            throw DriveApiException(
                "Cloud migration failed: ${failure.message}. $recovery No folders were deleted.",
                failure,
            )
        }
    }

    // A failed restore must not prevent recovery of the remaining acknowledged moves.
    @Suppress("TooGenericExceptionCaught", "SuspendFunSwallowedCancellation")
    private suspend fun rollbackMoves(
        token: String,
        move: CloudMove,
        moved: List<DriveFile>,
        failure: Exception,
    ): List<String> =
        withContext(NonCancellable) {
            val failures = mutableListOf<String>()
            for (file in moved.asReversed()) {
                try {
                    folders.moveFile(
                        token,
                        file.id,
                        addParentId = move.source,
                        removeParentId = move.destination,
                    )
                } catch (restoreFailure: Exception) {
                    failures += file.name
                    failure.addSuppressed(restoreFailure)
                }
            }
            failures
        }

    private data class CloudMove(
        val source: String,
        val destination: String,
        val oldPath: List<String>,
        val newPath: List<String>,
    )
}
