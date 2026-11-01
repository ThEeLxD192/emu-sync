package com.emusync.ui

import com.emusync.application.connection.CloudConnection
import com.emusync.application.library.GameOverrideChanges
import com.emusync.application.library.LoadLibrary
import com.emusync.application.library.SteamRegistration
import com.emusync.application.sync.SaveTargetMappingException
import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.update.UpdateInfo
import com.emusync.ui.connection.DriveConnectionController
import com.emusync.ui.library.EntryEditCallbacks
import com.emusync.ui.library.EntryEditor
import com.emusync.ui.library.GameItem
import com.emusync.ui.library.LibraryCallbacks
import com.emusync.ui.library.LibraryController
import com.emusync.ui.library.LibraryOperationCoordinator
import com.emusync.ui.library.LibraryServices
import com.emusync.ui.library.sync.GameSaveSetup
import com.emusync.ui.library.sync.SaveSetupRequest
import com.emusync.ui.updates.UpdateController
import com.emusync.ui.updates.UpdateServices
import java.io.File
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.withContext

/**
 * ViewModel / presentation state holder for the EmuSync app.
 *
 * Implements the Single Source of Truth (UDF) pattern via [AppUiState].
 */
// Preserve the presentation API used by the UI; editing, connection, library and update work
// is delegated to focused collaborators rather than expanding this facade.
@Suppress("TooManyFunctions")
class AppViewModel(
    services: LibraryServices,
    cloudConnection: CloudConnection,
    updates: UpdateServices,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO.limitedParallelism(1),
) {
    private val configManager = services.store
    private val steam = services.steam
    private val registrations = SteamRegistration(configManager, steam)
    private val openEmulator = services.openEmulator
    private val _uiState = MutableStateFlow(AppUiState())
    val uiState: StateFlow<AppUiState> = _uiState.asStateFlow()

    private val updateController =
        UpdateController(_uiState, updates.source, updates.files, updates.installer)
    private val operations = LibraryOperationCoordinator(_uiState)
    private val connection: DriveConnectionController =
        DriveConnectionController(_uiState, cloudConnection, ::checkSyncForEntry)
    private val library: LibraryController =
        LibraryController(
            _uiState,
            services.synchronizers,
            operations,
            LibraryCallbacks(
                services.loadGames,
                connection::refreshStoredConnection,
                ::applySaveSetup,
            ),
            services.saveTargets,
            ioDispatcher,
        )
    private val entries: EntryEditor =
        EntryEditor(
            _uiState,
            configManager,
            services.synchronizers,
            steam,
            EntryEditCallbacks(
                library::selectEntry,
                library::clearSelection,
                library::publishStatus,
            ),
            services.saveTargets,
        )

    private suspend fun applySaveSetup(item: GameItem, result: GameSaveSetup) {
        if (
            !entries.editGameOverride(
                item,
                GameOverrideChanges(result.targets, result.coverPath, result.title),
            )
        ) {
            throw SaveTargetMappingException(
                "Could not apply the save setup. Resolve the configuration error before playing."
            )
        }
        refreshSteamState()
    }

    /**
     * Loads the config.json and populates the initial state. Auto-syncs steamAppId with Steam's
     * shortcuts.vdf to ensure 100% consistency.
     */
    suspend fun loadConfig() =
        withContext(ioDispatcher) {
            if (operations.isBusy) return@withContext
            recoverUiFailure(
                action = {
                    _uiState.update { it.copy(status = AppStatus.Loading) }
                    val finalConfig = LoadLibrary(configManager, steam).load()

                    _uiState.update { it.copy(config = finalConfig, status = AppStatus.Idle) }

                    refreshSteamState()
                    // Auto-select the first entry
                    finalConfig.entries.firstOrNull()?.let { selectEntry(it) }
                },
                recover = { e ->
                    _uiState.update {
                        it.copy(status = AppStatus.Error("Failed to load config: ${e.message}"))
                    }
                },
            )
        }

    suspend fun selectEntry(entry: GameEntry) =
        withContext(ioDispatcher) { library.selectEntry(entry) }

    suspend fun checkSyncForEntry(entry: GameEntry) =
        withContext(ioDispatcher) { library.checkSyncForEntry(entry) }

    suspend fun syncEntryNow(entry: GameEntry) =
        withContext(ioDispatcher) { library.syncEntryNow(entry) }

    /** Reorders entries and persists their order without replacing unrelated concurrent edits. */
    suspend fun reorderEntries(fromIndex: Int, toIndex: Int) =
        withContext(ioDispatcher) { entries.reorderEntries(fromIndex, toIndex) }

    /** Applies the full entry order, preserving entries added while a reorder was pending. */
    suspend fun reorderEntriesList(newEntries: List<GameEntry>) =
        withContext(ioDispatcher) { entries.reorderEntriesList(newEntries) }

    fun completeSaveSetup(
        targets: List<SaveTarget>,
        title: String? = null,
        coverPath: String? = null,
        request: SaveSetupRequest? = null,
    ) = library.completeSaveSetup(targets, title, coverPath, request)

    suspend fun launchGame(item: GameItem) = withContext(ioDispatcher) { library.launchGame(item) }

    /** Opens an emulator interface directly for configuration, without launching a ROM. */
    suspend fun openEmulator(entry: EmulatorSystem) =
        withContext(ioDispatcher) {
            operations.runIfIdle {
                recoverUiFailure(
                    action = {
                        openEmulator.invoke(entry)
                    },
                    recover = { e ->
                        _uiState.update {
                            it.copy(
                                status =
                                    AppStatus.Error("Could not open ${entry.name}: ${e.message}")
                            )
                        }
                    },
                )
            }
        }

    /** Adds a new entry to config.json and selects it. */
    suspend fun addEntry(entry: GameEntry) =
        withContext(ioDispatcher) {
            entries.addEntry(entry)
            refreshSteamState()
        }

    /** Edits an entry only when no game or save-changing operation is active. */
    suspend fun editEntry(oldEntry: GameEntry, newEntry: GameEntry): Boolean =
        withContext(ioDispatcher) {
            operations.runIfIdle {
                entries.editEntry(oldEntry, newEntry).also { refreshSteamState() }
            } ?: false
        }

    /** Deletes an entry from config.json. */
    suspend fun deleteEntry(entry: GameEntry) =
        withContext(ioDispatcher) {
            operations.runIfIdle {
                entries.deleteEntry(entry)
                refreshSteamState()
            }
        }

    /** Updates the save targets and optional cover and title overrides for one game. */
    suspend fun editGameOverride(
        gameItem: GameItem,
        newTargets: List<SaveTarget>,
        newCoverPath: String? = null,
        newTitle: String? = null,
    ) =
        withContext(ioDispatcher) {
            operations.runIfIdle {
                entries.editGameOverride(
                    gameItem,
                    GameOverrideChanges(newTargets, newCoverPath, newTitle),
                )
                refreshSteamState()
            }
        }

    /** Updates the app status. */
    fun setStatus(status: AppStatus) {
        library.publishStatus(status)
    }

    // ── Steam Shortcut Management ────────────────────────────────

    /** Reads cached state only; Compose never consults the filesystem. */
    fun isSteamAvailable(): Boolean = _uiState.value.steamAvailable

    fun isSteamRegistered(entry: GameEntry): Boolean =
        entry.name in _uiState.value.steamRegisteredEntries

    private fun refreshSteamState() {
        val available = steam.isAvailable()
        val registered =
            _uiState.value.config
                ?.entries
                .orEmpty()
                .filter { steam.isRegistered(it) }
                .map { it.name }
                .toSet()
        _uiState.update { it.copy(steamAvailable = available, steamRegisteredEntries = registered) }
    }

    suspend fun registerInSteam(entry: GameEntry): String =
        withContext(ioDispatcher) {
            operations.runIfIdle {
                recoverUiFailure(
                    action = {
                        entries.publish(entry, registrations.register(entry))
                        refreshSteamState()
                        "\"${entry.name}\" registered in Steam! Restart Steam to see it in your library."
                    },
                    recover = { e -> "Failed to register in Steam: ${e.message}" },
                )
            } ?: "Wait for the current game or synchronization to finish."
        }

    suspend fun unregisterFromSteam(entry: GameEntry) =
        withContext(ioDispatcher) {
            operations.runIfIdle {
                recoverUiFailure(
                    action = {
                        entries.publish(entry, registrations.unregister(entry))
                        refreshSteamState()
                    },
                    recover = { e ->
                        _uiState.update {
                            it.copy(
                                status =
                                    AppStatus.Error("Failed to unregister from Steam: ${e.message}")
                            )
                        }
                    },
                )
            }
            Unit
        }

    // ── Google Drive Authentication ──────────────────────────────

    /** Saves OAuth credentials and optionally starts interactive authorization. */
    suspend fun setupGoogleDrive(
        clientId: String,
        clientSecret: String,
        connectNow: Boolean = true,
    ): String = withContext(ioDispatcher) { connection.setup(clientId, clientSecret, connectNow) }

    /** Clears the persisted refresh token and all visible linked-account sync indicators. */
    suspend fun unlinkGoogleDrive(): String = withContext(ioDispatcher) { connection.unlink() }

    /** Authorizes the current connection revision and refreshes the selected entry's status. */
    suspend fun loginGoogleDrive(): String = withContext(ioDispatcher) { connection.login() }

    // ── In-App Updates ───────────────────────────────────────────

    suspend fun checkForUpdates(manual: Boolean = false) =
        withContext(ioDispatcher) { updateController.checkForUpdates(manual) }

    fun dismissUpdateDialog() {
        _uiState.update { it.copy(showUpdateDialog = false) }
    }

    fun showUpdateDialog() {
        _uiState.update { it.copy(showUpdateDialog = true) }
    }

    suspend fun downloadAndApplyUpdate(info: UpdateInfo) =
        withContext(ioDispatcher) { updateController.download(info) }

    suspend fun restartApp(file: File): Boolean =
        withContext(ioDispatcher) { updateController.restart(file) }

    fun clearStatus() {
        if (operations.isBusy) return
        _uiState.update { it.copy(status = AppStatus.Idle) }
    }
}
