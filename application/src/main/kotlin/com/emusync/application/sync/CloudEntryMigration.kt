package com.emusync.application.sync

import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.SaveTarget as ConfiguredSaveTarget
import com.emusync.domain.library.effectiveCloudFolder
import com.emusync.domain.operation.recoverOperationFailure
import com.emusync.domain.sync.SyncEvent
import java.util.logging.Level

internal class CloudEntryMigration(
    private val context: SyncContext,
    private val paths: SaveTargetResolver,
    private val manual: ManualFileSync,
) {
    private val logger
        get() = context.logger

    private val search
        get() = context.search

    private val folders
        get() = context.folders

    private val folderMigrator
        get() = context.folderMigrator

    private val oauthFlow
        get() = context.oauthFlow

    private val config
        get() = context.config

    private val onStatus
        get() = context.onStatus

    /**
     * Handles renaming a game's cloud folder on Google Drive when the user changes its title.
     *
     * 1. If old folder exists and new folder does not: renames/moves the old folder to the new
     *    name.
     * 2. If new folder already exists (e.g. from another device):
     *     - Refuses duplicate names and moves non-conflicting files with rollback on failure.
     *     - Keeps the old folder as a recovery location.
     *     - Performs conflict check between local save files and cloud saves in new folder.
     *     - Prompts user with [SyncEvent.Conflict] if needed so they decide local vs cloud.
     */
    suspend fun renameGameCloudData(
        entry: GameEntry,
        romName: String?,
        oldTitle: String,
        newTitle: String,
        localSaveTargets: List<ConfiguredSaveTarget>,
    ): Boolean {
        val oldCloudPath = paths.getCloudPathSegments(entry, romName, oldTitle)
        val newCloudPath = paths.getCloudPathSegments(entry, romName, newTitle)

        if (oldCloudPath == newCloudPath || config.cloudCredentials?.refreshToken.isNullOrBlank())
            return true

        return recoverOperationFailure(
            action = {
                onStatus(SyncEvent.Syncing("Checking cloud folder for $newTitle..."))
                val token = authorizeRename()
                if (token == null) false
                else
                    renameFolders(
                        token,
                        entry,
                        newTitle,
                        localSaveTargets,
                        oldCloudPath to newCloudPath,
                    )
            },
            recover = { e ->
                onStatus(SyncEvent.Error("Cloud rename error: ${e.message}"))
                false
            },
        )
    }

    private suspend fun renameFolders(
        token: String,
        entry: GameEntry,
        newTitle: String,
        localSaveTargets: List<ConfiguredSaveTarget>,
        cloudPaths: Pair<List<String>, List<String>>,
    ): Boolean {
        val (oldCloudPath, newCloudPath) = cloudPaths
        val oldFolderId = folders.findPath(token, oldCloudPath)
        val newFolderId = folders.findPath(token, newCloudPath)

        return when {
            // Case 1: Old folder exists, but new folder does NOT exist yet -> Rename old
            // folder!
            oldFolderId != null && newFolderId == null -> {
                onStatus(SyncEvent.Syncing("Renaming cloud folder to $newTitle..."))
                folderMigrator.migrate(token, oldCloudPath, newCloudPath)
                onStatus(SyncEvent.Idle)
                true
            }

            // Case 2: New folder ALREADY exists! (with or without old folder) -> Check
            // conflict
            // and let user decide!
            newFolderId != null -> {
                onStatus(SyncEvent.Syncing("Checking cloud files in $newTitle..."))

                // Preflight duplicates before moving anything or changing local saves.
                if (oldFolderId != null && oldFolderId != newFolderId) {
                    folderMigrator.migrate(token, oldCloudPath, newCloudPath)
                }

                // Now check conflict between local save paths and newFolderId
                syncRenamedFolder(token, entry, newTitle, localSaveTargets, newFolderId)

                onStatus(SyncEvent.Idle)
                true
            }

            // Case 3: Neither old nor new folder exists in Drive
            else -> {
                onStatus(SyncEvent.Idle)
                true
            }
        }
    }

    private suspend fun syncRenamedFolder(
        token: String,
        entry: GameEntry,
        newTitle: String,
        localSaveTargets: List<ConfiguredSaveTarget>,
        newFolderId: String,
    ) {
        val cloudFiles = search.listFilesInFolder(token, newFolderId)
        context.mapping.validateSaveTargetMappings(localSaveTargets, cloudFiles)
        for (target in localSaveTargets) {
            val savePath = context.files.path(target.path)
            val reserved = context.mapping.reservedCloudNames(target, localSaveTargets, cloudFiles)
            if (context.mapping.isDirectory(target, cloudFiles, localSaveTargets)) {
                manual.syncDirectoryManual(
                    token,
                    entry,
                    savePath,
                    newFolderId,
                    context.mapping.cloudFilesForSaveTarget(
                        target.type,
                        savePath,
                        projectDirectoryFiles(cloudFiles, target.cloudPath),
                        reserved,
                    ),
                    newTitle,
                    reserved,
                    target.cloudPath,
                )
            } else {
                manual.syncFileManual(
                    token,
                    entry,
                    savePath,
                    newFolderId,
                    cloudFiles,
                    newTitle,
                    context.mapping.fileCloudName(target),
                )
            }
        }
    }

    private suspend fun authorizeRename(): String? =
        recoverOperationFailure(
            action = { oauthFlow.authorize(config, allowInteractive = false) },
            recover = { failure ->
                onStatus(SyncEvent.Error("Drive authentication failed: ${failure.message}"))
                null
            },
        )

    /** Migrates only this entry's folder, using the configuration before the edit. */
    suspend fun renameEntryCloudData(
        oldEntry: GameEntry,
        newEntry: GameEntry,
    ): Boolean {
        val oldPath = entryPath(oldEntry)
        val newPath = entryPath(newEntry)
        return when {
            oldPath == newPath -> true
            !canMigrateEntry(oldEntry, oldPath) -> false
            config.cloudCredentials?.refreshToken.isNullOrBlank() -> true
            else -> migrateEntry(newEntry, oldPath, newPath)
        }
    }

    private fun entryPath(entry: GameEntry): List<String> =
        when (entry) {
            is NativePCGame -> paths.getCloudPathSegments(entry)
            is EmulatorSystem -> listOf(entry.effectiveCloudFolder)
        }

    private fun canMigrateEntry(oldEntry: GameEntry, oldPath: List<String>): Boolean {
        val entryIndex =
            config.entries.indexOfFirst { it === oldEntry }.takeIf { it >= 0 }
                ?: config.entries.indexOf(oldEntry)
        val shared =
            config.entries
                .filterIndexed { index, _ -> index != entryIndex }
                .any { other ->
                    entryPath(other) == oldPath ||
                        (oldPath.size == 1 && other.effectiveCloudFolder == oldPath.first()) ||
                        paths.getSaveTargets(other).any { it.cloudPathSegments == oldPath }
                }
        if (shared) {
            onStatus(
                SyncEvent.Error(
                    "The cloud folder '${oldPath.joinToString("/")}' is shared with another entry. " +
                        "Keep the existing cloud folder when editing this entry to protect their saves."
                )
            )
            return false
        }

        return true
    }

    private suspend fun migrateEntry(
        newEntry: GameEntry,
        oldPath: List<String>,
        newPath: List<String>,
    ): Boolean {
        return recoverOperationFailure(
            action = {
                onStatus(SyncEvent.Syncing("Moving cloud saves for ${newEntry.name}..."))
                val token = oauthFlow.authorize(config, allowInteractive = false)
                folderMigrator.migrate(token, oldPath, newPath)
                onStatus(SyncEvent.Idle)
                true
            },
            recover = { e ->
                logger.log(Level.WARNING, "Cloud entry migration failed", e)
                onStatus(SyncEvent.Error("Cloud rename error: ${e.message}"))
                false
            },
        )
    }
}
