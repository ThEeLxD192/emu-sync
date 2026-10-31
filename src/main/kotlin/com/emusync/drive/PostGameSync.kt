package com.emusync.drive

import com.emusync.model.SaveTarget as ConfiguredSaveTarget
import com.emusync.ui.AppStatus
import com.emusync.ui.GameItem
import java.io.File
import java.util.logging.Level

internal class PostGameSync(
    private val context: SyncContext,
    private val paths: SaveTargetResolver,
    manual: ManualFileSync,
) {
    private val saves = PostSaveSync(context, manual)

    private data class UploadSession(
        val token: String,
        val cloudPath: List<String>,
        val cloudFiles: List<DriveFile>,
    )

    private class UploadSummary(completed: Int, blocked: List<String>) {
        var completedCount = completed
        val configured = mutableListOf<ConfiguredSaveTarget>()
        val files = mutableListOf<SaveUploadTarget>()
        val blockedReasons = blocked.toMutableList()
        var firstFailureReason: String? = null

        fun accept(result: PostSyncResult) {
            completedCount += result.completedCount
            files += result.failedTargets
            blockedReasons += result.blockedReasons
            firstFailureReason =
                firstFailureReason ?: result.failedTargets.firstNotNullOfOrNull { it.failureReason }
        }
    }

    suspend fun uploadAfterPlay(item: GameItem, syncTargets: List<ConfiguredSaveTarget>): Boolean =
        uploadBatch(item, syncTargets, null, UploadSummary(0, emptyList()), syncTargets)

    private suspend fun uploadBatch(
        item: GameItem,
        syncTargets: List<ConfiguredSaveTarget>,
        retryTargets: List<SaveUploadTarget>?,
        summary: UploadSummary,
        mappingTargets: List<ConfiguredSaveTarget>,
    ): Boolean {
        context.onStatus(AppStatus.Syncing("Uploading saves for ${item.effectiveTitle}..."))
        recoverDriveFailure(
            action = {
                // Games can outlive an access token. Authorize again before uploading.
                val token =
                    context.oauthFlow.authorize(
                        context.config,
                        context.configManager,
                        allowInteractive = false,
                    )
                val cloudPath = paths.getCloudPathSegments(item)
                val folderId = context.folders.ensurePath(token, cloudPath)
                val cloudFiles =
                    canonicalCloudFiles(context.search.listFilesInFolder(token, folderId))
                validateSaveTargetMappings(mappingTargets, cloudFiles)
                val session = UploadSession(token, cloudPath, cloudFiles)
                uploadRetries(item, retryTargets.orEmpty(), session, summary)
                uploadConfigured(item, syncTargets, mappingTargets, session, summary)
            },
            recover = { failure ->
                if (failure is SaveTargetMappingException) {
                    context.onStatus(AppStatus.Error(failure.message ?: "Invalid save mapping"))
                    return false
                }
                summary.configured += syncTargets.filterNot { it in summary.configured }
                summary.files += retryTargets.orEmpty().filterNot { it in summary.files }
                summary.firstFailureReason = summary.firstFailureReason ?: failure.message
            },
        )
        return finishUpload(item, summary, mappingTargets)
    }

    private suspend fun uploadRetries(
        item: GameItem,
        targets: List<SaveUploadTarget>,
        session: UploadSession,
        summary: UploadSummary,
    ) {
        for (target in targets.filter { it.file.exists() }) {
            recoverDriveFailure(
                action = {
                    summary.accept(
                        saves.sync(
                            session.token,
                            item.entry,
                            target.file,
                            target.type,
                            session.cloudPath,
                            driveNameOverride = target.driveName,
                            knownFileSiblingNames = target.reservedDriveNames,
                        )
                    )
                },
                recover = { failure ->
                    summary.files += target
                    summary.firstFailureReason = summary.firstFailureReason ?: failure.message
                },
            )
        }
    }

    private suspend fun uploadConfigured(
        item: GameItem,
        targets: List<ConfiguredSaveTarget>,
        mappingTargets: List<ConfiguredSaveTarget>,
        session: UploadSession,
        summary: UploadSummary,
    ) {
        for (target in targets) {
            val local = File(target.path)
            if (!local.exists()) continue
            val siblings = fileSiblingCloudNames(target, mappingTargets, session.cloudFiles)
            val reserved = reservedCloudNames(target, mappingTargets, session.cloudFiles)
            recoverDriveFailure(
                action = {
                    summary.accept(
                        saves.sync(
                            session.token,
                            item.entry,
                            local,
                            target.type,
                            session.cloudPath,
                            knownFileSiblingNames = siblings,
                            reservedSiblingNames = reserved,
                            cloudPath = target.cloudPath,
                        )
                    )
                },
                recover = { failure ->
                    summary.configured += target
                    summary.firstFailureReason = summary.firstFailureReason ?: failure.message
                },
            )
        }
    }

    private fun finishUpload(
        item: GameItem,
        summary: UploadSummary,
        mappingTargets: List<ConfiguredSaveTarget>,
    ): Boolean {
        val blocked = summary.blockedReasons.distinct()
        return if (summary.configured.isEmpty() && summary.files.isEmpty()) {
            if (blocked.isNotEmpty()) context.onStatus(AppStatus.Error(blocked.joinToString("\n")))
            else context.onStatus(AppStatus.Idle)
            blocked.isEmpty()
        } else {
            reportFailure(item, summary, mappingTargets, blocked)
            false
        }
    }

    private fun reportFailure(
        item: GameItem,
        summary: UploadSummary,
        mappingTargets: List<ConfiguredSaveTarget>,
        blocked: List<String>,
    ) {
        val reason = buildList {
            summary.firstFailureReason?.takeIf { it.isNotBlank() }?.let(::add)
            addAll(blocked)
        }
            .distinct()
            .joinToString("\n")
            .ifBlank { "Unknown Drive error" }
        context.logger.log(
            Level.WARNING,
            "Post-game Drive sync failed for {0}: {1}",
            arrayOf(item.effectiveTitle, reason),
        )
        context.onStatus(
            AppStatus.PostSyncFailed(
                gameName = item.effectiveTitle,
                reason = reason,
                completedSaveCount = summary.completedCount,
                pendingSaveCount = summary.configured.size + summary.files.size + blocked.size,
                onRetry = {
                    uploadBatch(
                        item,
                        summary.configured,
                        summary.files,
                        UploadSummary(summary.completedCount, blocked),
                        mappingTargets,
                    )
                },
            )
        )
    }
}
