package com.emusync.application.sync

import com.emusync.domain.config.AppConfig
import com.emusync.domain.sync.PostSyncFailure
import com.emusync.domain.sync.SaveConflict
import com.emusync.domain.sync.SyncDecision
import com.emusync.domain.sync.SyncEvent
import java.util.logging.Logger
import kotlinx.coroutines.CompletableDeferred

/** Borrowed capabilities for one operation. The composition owns their resources. */
internal class SyncContext(
    val config: AppConfig,
    services: SyncServices,
    val onStatus: (SyncEvent) -> Unit,
) {
    val files = services.files
    val search = services.search
    val folders = services.folders
    val trackedSync = services.trackedSync
    val oauthFlow = services.authorization
    val runner = services.runner
    val logger: Logger = Logger.getLogger(SyncOrchestrator::class.java.name)
    val folderMigrator = CloudFolderMigrator(folders, search)
    val mapping = SaveTargetMapping(files)
    private var pendingConflict: Pair<SaveConflict, CompletableDeferred<SyncDecision>>? = null
    private var pendingRetry: Pair<PostSyncFailure, suspend () -> Unit>? = null

    suspend fun requestConflict(request: SaveConflict): SyncDecision {
        check(pendingConflict == null) { "A conflict is already pending" }
        val response = CompletableDeferred<SyncDecision>()
        pendingConflict = request to response
        try {
            onStatus(SyncEvent.Conflict(request))
            return response.await()
        } finally {
            if (pendingConflict?.first === request) pendingConflict = null
            response.cancel()
        }
    }

    fun resolveConflict(request: SaveConflict, decision: SyncDecision) {
        pendingConflict?.takeIf { it.first === request }?.second?.complete(decision)
    }

    fun reportPostSyncFailure(failure: PostSyncFailure, retry: suspend () -> Unit) {
        pendingRetry = failure to retry
        onStatus(SyncEvent.PostSyncFailed(failure))
    }

    suspend fun retryPostSync(failure: PostSyncFailure) {
        val retry = pendingRetry?.takeIf { it.first === failure } ?: return
        pendingRetry = null
        retry.second()
    }
}
