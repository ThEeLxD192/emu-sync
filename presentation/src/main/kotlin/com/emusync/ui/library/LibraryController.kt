package com.emusync.ui.library

import com.emusync.application.library.EntrySyncChecks
import com.emusync.domain.config.AppConfig
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetAccess
import com.emusync.domain.sync.SaveSynchronizerFactory
import com.emusync.ui.AppStatus
import com.emusync.ui.AppUiState
import com.emusync.ui.library.sync.CloudSyncStatus
import com.emusync.ui.library.sync.GameSaveSetup
import com.emusync.ui.library.sync.SaveSetupRequest
import com.emusync.ui.library.sync.createSaveSynchronizer
import com.emusync.ui.library.sync.playWithSync
import com.emusync.ui.library.sync.toUiStatus
import com.emusync.ui.recoverUiFailure
import com.emusync.ui.restoreUiOnCancellation
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.withContext

/**
 * Coordinates library loading, save synchronization and game sessions independently of
 * presentation.
 */
internal class LibraryController(
    private val state: MutableStateFlow<AppUiState>,
    private val synchronizers: SaveSynchronizerFactory,
    private val operations: LibraryOperationCoordinator,
    private val callbacks: LibraryCallbacks,
    private val saveTargets: SaveTargetAccess,
    private val ioDispatcher: CoroutineDispatcher,
) {
    private val loadGames
        get() = callbacks.loadGames

    private val refreshConnection
        get() = callbacks.refreshConnection

    private val saveGameSetup
        get() = callbacks.saveGameSetup

    private val selectionGeneration = AtomicLong()
    private val checks = EntrySyncChecks(operations.operations, synchronizers)
    private val setupLock = Any()
    private var pendingSetup: CompletableDeferred<GameSaveSetup>? = null

    suspend fun selectEntry(entry: GameEntry) {
        val request = selectionGeneration.incrementAndGet()
        val linked = state.value.config?.cloudCredentials?.refreshToken?.isNotBlank() == true
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
        val request = checks.begin(entry, cfg)
        if (cfg.cloudCredentials?.refreshToken.isNullOrBlank()) {
            state.update {
                it.copy(
                    entrySyncStatus =
                        it.entrySyncStatus + (entry.name to CloudSyncStatus.NOT_CONFIGURED)
                )
            }
            return
        }
        fun isCurrent(current: AppUiState) = checks.isCurrent(request, current.config)
        state.update {
            if (isCurrent(it))
                it.copy(
                    entrySyncStatus = it.entrySyncStatus + (entry.name to CloudSyncStatus.CHECKING)
                )
            else it
        }
        restoreUiOnCancellation(
            action = {
                val status = checks.read(request).toUiStatus()
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
                createSaveSynchronizer(synchronizers, cfg, ::publishStatus).syncEntry(entry)
            refreshConnection()
            if (state.value.config?.cloudCredentials == cfg.cloudCredentials) {
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
            createSaveSynchronizer(synchronizers, cfg, ::publishStatus).playWithSync(
                item,
                saveTargets,
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
                    withContext(ioDispatcher) {
                        operations.runIfIdle {
                            // A callback retained by an old dialog cannot replay a completed
                            // upload.
                            if (state.value.status === displayed) {
                                status.onRetry()
                                refreshConnection()
                            }
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
            pendingSetup?.complete(GameSaveSetup(targets, title, coverPath))
            pendingSetup = null
            state.update { it.copy(saveSetupRequest = null) }
        }

    private suspend fun requestSaveSetup(
        item: GameItem,
        notice: String?,
        names: List<String>,
    ): GameSaveSetup {
        val deferred = CompletableDeferred<GameSaveSetup>()
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
