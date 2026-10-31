package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.model.AppConfig
import com.emusync.runner.GameRunner
import com.emusync.ui.AppStatus
import io.ktor.client.HttpClient
import java.util.logging.Logger

/**
 * Shared services for one synchronization operation; lifecycle state lives in its collaborators.
 */
internal class SyncContext(
    client: HttpClient,
    val config: AppConfig,
    val configManager: ConfigManager,
    val onStatus: (AppStatus) -> Unit,
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
            onStatus(AppStatus.Syncing("$verb ${progress.fileName}$suffix"))
        }
    val folders = DriveFolders(client)
    val trackedSync = TrackedFileSync(transfer, SyncHistoryStore(configManager.configFile))
    val folderMigrator = CloudFolderMigrator(folders, search)
    val oauthFlow = OAuthFlow(client)
    val runner = GameRunner()
}
