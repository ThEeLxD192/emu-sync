package com.emusync.ui

import com.emusync.composition.composeSaveSynchronizer
import com.emusync.config.ConfigManager
import com.emusync.domain.config.AppConfig
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.sync.EntrySyncStatus
import com.emusync.domain.sync.SaveSetup
import com.emusync.domain.sync.SaveSetupResult
import com.emusync.domain.sync.SaveSynchronizer
import com.emusync.domain.sync.SyncEvent
import com.emusync.domain.sync.SyncGame
import io.ktor.client.HttpClient
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter

private val syncDateFormatter =
    DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss").withZone(ZoneId.systemDefault())

internal fun formatSyncTimestamp(epochMs: Long): String =
    syncDateFormatter.format(Instant.ofEpochMilli(epochMs))

internal fun GameItem.toSyncGame() =
    SyncGame(name, entry, romFile?.path, effectiveTitle, effectiveSaveTargets)

internal fun EntrySyncStatus.toUiStatus(): CloudSyncStatus =
    when (this) {
        EntrySyncStatus.IDLE -> CloudSyncStatus.IDLE
        EntrySyncStatus.IN_SYNC -> CloudSyncStatus.IN_SYNC
        EntrySyncStatus.OUT_OF_SYNC -> CloudSyncStatus.OUT_OF_SYNC
        EntrySyncStatus.CONFLICT -> CloudSyncStatus.CONFLICT
        EntrySyncStatus.NOT_CONFIGURED -> CloudSyncStatus.NOT_CONFIGURED
        EntrySyncStatus.ERROR -> CloudSyncStatus.ERROR
    }

/** Adapt neutral events into UI state, keeping response callbacks at the presentation boundary. */
internal fun createSaveSynchronizer(
    client: HttpClient,
    config: AppConfig,
    configManager: ConfigManager,
    onStatus: (AppStatus) -> Unit,
): SaveSynchronizer {
    lateinit var sync: SaveSynchronizer
    sync =
        composeSaveSynchronizer(client, config, configManager) { event ->
            onStatus(event.toUiStatus(sync))
        }
    return sync
}

internal fun SyncEvent.toUiStatus(sync: SaveSynchronizer): AppStatus =
    when (this) {
        SyncEvent.Idle -> AppStatus.Idle
        SyncEvent.Playing -> AppStatus.Playing
        is SyncEvent.Syncing -> AppStatus.Syncing(message)
        is SyncEvent.Error -> AppStatus.Error(message)
        is SyncEvent.Conflict ->
            AppStatus.Conflict(
                localDate = formatSyncTimestamp(request.localModifiedTimeMs),
                cloudDate = formatSyncTimestamp(request.cloudModifiedTimeMs),
                gameName = request.gameName,
                onResolve = { sync.resolveConflict(request, it) },
            )
        is SyncEvent.PostSyncFailed ->
            AppStatus.PostSyncFailed(
                gameName = failure.gameName,
                reason = failure.reason,
                completedSaveCount = failure.completedSaveCount,
                pendingSaveCount = failure.pendingSaveCount,
                onRetry = { sync.retryPostSync(failure) },
            )
    }

/** Save dialogs may edit title/cover as well; only resolved sync data crosses into the service. */
internal suspend fun SaveSynchronizer.playWithSync(
    item: GameItem,
    resolveConfiguredItem: (GameItem) -> GameItem = { it },
    onRequestSaveSetup: suspend (GameItem, String?, List<String>) -> List<SaveTarget> = { _, _, _ ->
        emptyList()
    },
) {
    var current = item
    playWithSync(
        item.toSyncGame(),
        SaveSetup { request ->
            val notice =
                if (request.cloudFileNames.isEmpty()) null
                else
                    "Cloud save data was found for this game. Choose one destination folder; only these listed files will be synchronized for this game."
            val targets = onRequestSaveSetup(current, notice, request.cloudFileNames)
            current = resolveConfiguredItem(current)
            SaveSetupResult(current.toSyncGame(), targets)
        },
    )
}
