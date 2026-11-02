package com.emusync.ui

import com.emusync.application.sync.SyncOrchestrator
import com.emusync.application.sync.SyncServices
import com.emusync.config.ConfigManager
import com.emusync.domain.config.AppConfig
import com.emusync.domain.config.CloudAuthorization
import com.emusync.domain.sync.SaveSynchronizer
import com.emusync.domain.sync.SyncEvent
import com.emusync.drive.DriveFileListing
import com.emusync.drive.DriveFolders
import com.emusync.drive.DriveSearch
import com.emusync.drive.DriveTransfer
import com.emusync.drive.OAuthFlow
import com.emusync.drive.TransferDirection
import com.emusync.local.saves.DesktopSaveAccess
import com.emusync.local.saves.SyncHistoryStore
import com.emusync.local.saves.TrackedFileSync
import com.emusync.runner.DesktopBrowserLauncher
import com.emusync.runner.GameRunner
import com.emusync.steam.DesktopSteamLibrary
import com.emusync.steam.SteamShortcutManager
import io.ktor.client.HttpClient

internal fun testSaveSynchronizer(
    client: HttpClient,
    config: AppConfig,
    store: ConfigManager,
    onStatus: (SyncEvent) -> Unit,
): SaveSynchronizer {
    val transfer =
        DriveTransfer(client) { progress ->
            val verb =
                if (progress.direction == TransferDirection.DOWNLOAD) "Downloading" else "Uploading"
            val percent =
                progress.totalBytes
                    ?.takeIf { it > 0L }
                    ?.let { ((progress.transferredBytes * 100L) / it).coerceIn(0L, 100L) }
            val suffix = percent?.let { " ($it%)" }.orEmpty()
            onStatus(SyncEvent.Syncing("$verb ${progress.fileName}$suffix"))
        }
    val oauth = OAuthFlow(client, credentialStore = store, browser = DesktopBrowserLauncher())
    return SyncOrchestrator(
        config,
        SyncServices(
            DesktopSaveAccess(),
            DriveFileListing(DriveSearch(client)),
            DriveFolders(client),
            TrackedFileSync(transfer, SyncHistoryStore(store.configFile)),
            CloudAuthorization { cfg, interactive -> oauth.authorize(cfg, interactive) },
            GameRunner(DesktopSteamLibrary(SteamShortcutManager())),
        ),
        onStatus,
    )
}
