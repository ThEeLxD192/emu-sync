package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.domain.execution.GameLauncher
import com.emusync.domain.sync.CloudFolderStore
import com.emusync.domain.sync.PostSyncFailure
import com.emusync.domain.sync.SaveConflict
import com.emusync.domain.sync.SyncDecision
import com.emusync.domain.sync.SyncEvent
import com.emusync.model.AppConfig
import com.emusync.runner.GameRunner
import io.ktor.client.HttpClient
import java.util.logging.Logger
import kotlinx.coroutines.CompletableDeferred

/**
 * Shared services for one synchronization operation; lifecycle state lives in its collaborators.
 */
internal class SyncContext(
    client: HttpClient,
    val config: AppConfig,
    val configManager: ConfigManager,
    val onStatus: (SyncEvent) -> Unit,
) {
    val logger: java.util.logging.Logger = Logger.getLogger(SyncOrchestrator::class.java.name)
    val search = DriveSearch(client)
    val transfer =
        DriveTransfer(client) { progress ->
            val verb =
                if (progress.direction == TransferProgress.Direction.DOWNLOAD) "Downloading"
                else "Uploading"
            val percent =
                progress.totalBytes
                    ?.takeIf { it > 0L }
                    ?.let { ((progress.transferredBytes * 100L) / it).coerceIn(0L, 100L) }
            val suffix = percent?.let { " ($it%)" }.orEmpty()
            onStatus(SyncEvent.Syncing("$verb ${progress.fileName}$suffix"))
        }
    val folders: CloudFolderStore = DriveFolders(client)
    val trackedSync = TrackedFileSync(transfer, SyncHistoryStore(configManager.configFile))
    val folderMigrator = CloudFolderMigrator(folders, search)
    val oauthFlow = OAuthFlow(client)
    val runner: GameLauncher = GameRunner()
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
