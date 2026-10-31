package com.emusync.ui

import com.emusync.config.ConfigManager
import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.SaveTarget
import com.emusync.model.AppConfig
import com.emusync.scanner.scanRoms
import io.ktor.client.HttpClient
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update

internal suspend fun loadLibraryGames(entry: GameEntry): List<GameItem> =
    when (entry) {
        is EmulatorSystem ->
            scanRoms(entry.romsDirectory, entry.extensions).map {
                GameItem(it.name, entry, it.file)
            }
        is NativePCGame -> listOf(GameItem(entry.name, entry))
    }

internal data class LibraryCallbacks(
    val loadGames: suspend (GameEntry) -> List<GameItem>,
    val refreshConnection: () -> Unit,
    val saveGameSetup: suspend (GameItem, AppViewModel.SaveSetupResult) -> Unit,
)

/**
 * Coordinates library loading, save synchronization and game sessions independently of
 * presentation.
 */
internal class LibraryController(
    private val state: MutableStateFlow<AppUiState>,
    private val configManager: ConfigManager,
    private val client: HttpClient,
    private val operations: LibraryOperationCoordinator,
    private val callbacks: LibraryCallbacks,
) {
    private val loadGames
        get() = callbacks.loadGames

    private val refreshConnection
        get() = callbacks.refreshConnection

    private val saveGameSetup
        get() = callbacks.saveGameSetup

    private val selectionGeneration = AtomicLong()
    private val checkGeneration = AtomicLong()
    private val checks = ConcurrentHashMap<String, Long>()
    private val setupLock = Any()
    private var pendingSetup: CompletableDeferred<AppViewModel.SaveSetupResult>? = null

    suspend fun selectEntry(entry: GameEntry) {
        val request = selectionGeneration.incrementAndGet()
        val linked = state.value.config?.googleDrive?.refreshToken?.isNotBlank() == true
        state.update {
            it.copy(
                isLoading = true,
                selectedEntry = entry,
                gameItems = emptyList(),
                entrySyncStatus =
                    it.entrySyncStatus + (entry.name to initialSyncStatus(it, entry, linked)),
            )
        }
        restoreUiOnCancellation(
            action = {
                recoverUiFailure(
                    action = {
                        val items = loadGames(entry)
                        state.update {
                            if (selectionGeneration.get() == request && it.selectedEntry == entry)
                                it.copy(isLoading = false, gameItems = items)
                            else it
                        }
                        if (
                            selectionGeneration.get() == request &&
                                state.value.selectedEntry == entry &&
                                linked
                        )
                            checkSyncForEntry(entry)
                    },
                    recover = { failure ->
                        finishFailedScan(
                            request,
                            entry,
                            "Could not load ${entry.name}: ${failure.message}",
                        )
                    },
                )
            },
            restore = { finishFailedScan(request, entry, null) },
        )
    }

    private fun initialSyncStatus(
        current: AppUiState,
        entry: GameEntry,
        linked: Boolean,
    ): CloudSyncStatus =
        when {
            !linked -> CloudSyncStatus.NOT_CONFIGURED
            operations.isBusy -> current.entrySyncStatus[entry.name] ?: CloudSyncStatus.IDLE
            else -> CloudSyncStatus.CHECKING
        }

    private fun finishFailedScan(request: Long, entry: GameEntry, errorMessage: String?) {
        state.update {
            if (selectionGeneration.get() != request || it.selectedEntry != entry) it
            else
                it.copy(
                    isLoading = false,
                    status = errorMessage?.let(AppStatus::Error) ?: it.status,
                )
        }
    }

    /** Invalidates scans when the library clears or replaces the current selection. */
    fun clearSelection() {
        selectionGeneration.incrementAndGet()
        state.update { it.copy(selectedEntry = null, gameItems = emptyList(), isLoading = false) }
    }

    suspend fun checkSyncForEntry(entry: GameEntry) {
        val config = state.value.config
        if (!operations.isBusy && config != null && config.entries.any { it == entry })
            checkSyncStatus(entry, config)
    }

    private suspend fun checkSyncStatus(entry: GameEntry, cfg: AppConfig) {
        val request = checkGeneration.incrementAndGet()
        checks[entry.name] = request
        val revision = operations.revision
        if (cfg.googleDrive?.refreshToken.isNullOrBlank()) {
            state.update {
                it.copy(
                    entrySyncStatus =
                        it.entrySyncStatus + (entry.name to CloudSyncStatus.NOT_CONFIGURED)
                )
            }
            return
        }
        fun isCurrent(current: AppUiState) =
            checks[entry.name] == request &&
                operations.revision == revision &&
                !operations.isBusy &&
                current.config?.googleDrive == cfg.googleDrive &&
                current.config.entries.any { it == entry }
        state.update {
            if (isCurrent(it))
                it.copy(
                    entrySyncStatus = it.entrySyncStatus + (entry.name to CloudSyncStatus.CHECKING)
                )
            else it
        }
        restoreUiOnCancellation(
            action = {
                val status =
                    createSaveSynchronizer(client, cfg, configManager) {}
                        .checkEntrySyncStatus(entry)
                        .toUiStatus()
                refreshConnection()
                state.update {
                    if (isCurrent(it))
                        it.copy(entrySyncStatus = it.entrySyncStatus + (entry.name to status))
                    else it
                }
            },
            restore = {
                state.update {
                    if (isCurrent(it))
                        it.copy(
                            entrySyncStatus =
                                it.entrySyncStatus + (entry.name to CloudSyncStatus.IDLE)
                        )
                    else it
                }
            },
        )
    }

    suspend fun syncEntryNow(entry: GameEntry) {
        val ran = operations.runIfIdle {
            val cfg = state.value.config ?: return@runIfIdle false
            if (cfg.entries.none { it == entry }) return@runIfIdle false
            state.update {
                it.copy(
                    entrySyncStatus = it.entrySyncStatus + (entry.name to CloudSyncStatus.SYNCING)
                )
            }
            val success =
                createSaveSynchronizer(client, cfg, configManager, ::publishStatus).syncEntry(entry)
            refreshConnection()
            if (state.value.config?.googleDrive == cfg.googleDrive) {
                state.update {
                    it.copy(
                        entrySyncStatus =
                            it.entrySyncStatus +
                                (entry.name to
                                    if (success) CloudSyncStatus.IN_SYNC else CloudSyncStatus.ERROR)
                    )
                }
            }
            true
        }
        if (ran == true) checkSyncForEntry(entry)
    }

    suspend fun launchGame(item: GameItem) {
        val ran = operations.runIfIdle {
            val cfg = state.value.config ?: return@runIfIdle false
            if (cfg.entries.none { it == item.entry }) return@runIfIdle false
            var configuredItem = item
            createSaveSynchronizer(client, cfg, configManager, ::publishStatus).playWithSync(
                item,
                resolveConfiguredItem = { configuredItem },
            ) { requested, notice, names ->
                val result = requestSaveSetup(requested, notice, names)
                if (
                    result.targets.isNotEmpty() || result.title != null || result.coverPath != null
                ) {
                    saveGameSetup(requested, result)
                    val entryName =
                        if (requested.entry is NativePCGame)
                            result.title?.trim()?.takeIf { it.isNotEmpty() } ?: requested.entry.name
                        else requested.entry.name
                    state.value.config
                        ?.entries
                        ?.find { it.name == entryName }
                        ?.let { configuredItem = requested.copy(entry = it) }
                }
                result.targets
            }
            refreshConnection()
            true
        }
        if (ran == true) {
            state.value.config
                ?.entries
                ?.find { it.name == item.entry.name }
                ?.let { checkSyncForEntry(it) }
        }
    }

    fun publishStatus(status: AppStatus) {
        if (status !is AppStatus.PostSyncFailed) {
            state.update { it.copy(status = status) }
            return
        }
        lateinit var displayed: AppStatus.PostSyncFailed
        displayed =
            status.copy(
                onRetry = {
                    operations.runIfIdle {
                        // A callback retained by an old dialog cannot replay a completed upload.
                        if (state.value.status === displayed) {
                            status.onRetry()
                            refreshConnection()
                        }
                    }
                }
            )
        state.update { it.copy(status = displayed) }
    }

    fun completeSaveSetup(
        targets: List<SaveTarget>,
        title: String?,
        coverPath: String?,
        request: SaveSetupRequest?,
    ) =
        synchronized(setupLock) {
            if (request != null && state.value.saveSetupRequest !== request) return@synchronized
            pendingSetup?.complete(AppViewModel.SaveSetupResult(targets, title, coverPath))
            pendingSetup = null
            state.update { it.copy(saveSetupRequest = null) }
        }

    private suspend fun requestSaveSetup(
        item: GameItem,
        notice: String?,
        names: List<String>,
    ): AppViewModel.SaveSetupResult {
        val deferred = CompletableDeferred<AppViewModel.SaveSetupResult>()
        val request = SaveSetupRequest(item, notice, names)
        synchronized(setupLock) {
            check(pendingSetup == null) { "A save setup is already pending" }
            pendingSetup = deferred
            state.update { it.copy(saveSetupRequest = request) }
        }
        try {
            return deferred.await()
        } finally {
            synchronized(setupLock) {
                if (pendingSetup === deferred) pendingSetup = null
                if (state.value.saveSetupRequest === request)
                    state.update { it.copy(saveSetupRequest = null) }
            }
            deferred.cancel()
        }
    }
}
