package com.emusync.ui

import com.emusync.config.ConfigManager
import com.emusync.drive.DriveClientFactory
import com.emusync.drive.OAuthFlow
import com.emusync.drive.SaveTargetMappingException
import com.emusync.model.EmulatorSystem
import com.emusync.model.GameEntry
import com.emusync.model.NativePCGame
import com.emusync.model.SaveTarget
import com.emusync.runner.GameRunner
import com.emusync.steam.SteamShortcutManager
import com.emusync.update.UpdateInfo
import com.emusync.update.UpdateManager
import io.ktor.client.HttpClient
import java.io.File
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/**
 * ViewModel / presentation state holder for the EmuSync app.
 *
 * Implements the Single Source of Truth (UDF) pattern via [AppUiState].
 */
// Preserve the presentation API used by the UI; editing, connection, library and update work
// is delegated to focused collaborators rather than expanding this facade.
@Suppress("TooManyFunctions")
class AppViewModel(
    private val configManager: ConfigManager,
    private val steamManager: SteamShortcutManager = SteamShortcutManager(),
    private val httpClient: HttpClient = DriveClientFactory.create(),
    private val updateManager: UpdateManager = UpdateManager(),
    private val gameRunner: GameRunner = GameRunner(),
    oauthFlow: OAuthFlow = OAuthFlow(httpClient),
    loadGames: suspend (GameEntry) -> List<GameItem> = ::loadLibraryGames,
) : AutoCloseable {
    private val _uiState = MutableStateFlow(AppUiState())
    val uiState: StateFlow<AppUiState> = _uiState.asStateFlow()

    data class SaveSetupResult(
        val targets: List<SaveTarget>,
        val title: String? = null,
        val coverPath: String? = null,
    )

    private val updates = UpdateController(_uiState, updateManager)
    private val operations = LibraryOperationCoordinator(_uiState)
    private val connection: DriveConnectionController =
        DriveConnectionController(_uiState, configManager, oauthFlow, ::checkSyncForEntry)
    private val library: LibraryController =
        LibraryController(
            _uiState,
            configManager,
            httpClient,
            operations,
            LibraryCallbacks(loadGames, connection::refreshStoredConnection, ::applySaveSetup),
        )
    private val entries: EntryEditor =
        EntryEditor(
            _uiState,
            configManager,
            httpClient,
            steamManager,
            EntryEditCallbacks(
                library::selectEntry,
                library::clearSelection,
                library::publishStatus,
            ),
        )

    private suspend fun applySaveSetup(item: GameItem, result: SaveSetupResult) {
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
    }

    /**
     * Loads the config.json and populates the initial state. Auto-syncs steamAppId with Steam's
     * shortcuts.vdf to ensure 100% consistency.
     */
    suspend fun loadConfig() {
        if (operations.isBusy) return
        recoverUiFailure(
            action = {
                _uiState.update { it.copy(status = AppStatus.Loading) }
                val cfg = configManager.load()

                val steamShortcuts = steamManager.migrateLegacyLaunchers(cfg.entries)
                var configChanged = false
                val synchronizedEntries =
                    cfg.entries.map { entry ->
                        val matchingShortcut = steamShortcuts.find {
                            it.appName == "[EmuSync] ${entry.name}"
                        }
                        if (
                            matchingShortcut != null && entry.steamAppId != matchingShortcut.appId
                        ) {
                            configChanged = true
                            when (entry) {
                                is EmulatorSystem -> entry.copy(steamAppId = matchingShortcut.appId)
                                is NativePCGame -> entry.copy(steamAppId = matchingShortcut.appId)
                            }
                        } else if (matchingShortcut == null && entry.steamAppId != null) {
                            configChanged = true
                            when (entry) {
                                is EmulatorSystem -> entry.copy(steamAppId = null)
                                is NativePCGame -> entry.copy(steamAppId = null)
                            }
                        } else {
                            entry
                        }
                    }

                val finalConfig =
                    if (configChanged) {
                        configManager.update { it.copy(entries = synchronizedEntries) }
                    } else {
                        cfg
                    }

                _uiState.update { it.copy(config = finalConfig, status = AppStatus.Idle) }

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

    suspend fun selectEntry(entry: GameEntry) = library.selectEntry(entry)

    suspend fun checkSyncForEntry(entry: GameEntry) = library.checkSyncForEntry(entry)

    suspend fun syncEntryNow(entry: GameEntry) = library.syncEntryNow(entry)

    /** Reorders entries and persists their order without replacing unrelated concurrent edits. */
    fun reorderEntries(fromIndex: Int, toIndex: Int) = entries.reorderEntries(fromIndex, toIndex)

    /** Applies the full entry order, preserving entries added while a reorder was pending. */
    fun reorderEntriesList(newEntries: List<GameEntry>) = entries.reorderEntriesList(newEntries)

    fun completeSaveSetup(
        targets: List<SaveTarget>,
        title: String? = null,
        coverPath: String? = null,
        request: SaveSetupRequest? = null,
    ) = library.completeSaveSetup(targets, title, coverPath, request)

    suspend fun launchGame(item: GameItem) = library.launchGame(item)

    /** Opens an emulator interface directly for configuration, without launching a ROM. */
    suspend fun openEmulator(entry: EmulatorSystem) {
        operations.runIfIdle {
            recoverUiFailure(
                action = {
                    gameRunner.openEmulator(entry)
                },
                recover = { e ->
                    _uiState.update {
                        it.copy(
                            status = AppStatus.Error("Could not open ${entry.name}: ${e.message}")
                        )
                    }
                },
            )
        }
    }

    /** Adds a new entry to config.json and selects it. */
    suspend fun addEntry(entry: GameEntry) = entries.addEntry(entry)

    /** Edits an entry only when no game or save-changing operation is active. */
    suspend fun editEntry(oldEntry: GameEntry, newEntry: GameEntry): Boolean =
        operations.runIfIdle { entries.editEntry(oldEntry, newEntry) } ?: false

    /** Deletes an entry from config.json. */
    suspend fun deleteEntry(entry: GameEntry) {
        operations.runIfIdle { entries.deleteEntry(entry) }
    }

    /** Updates the save targets and optional cover and title overrides for one game. */
    suspend fun editGameOverride(
        gameItem: GameItem,
        newTargets: List<SaveTarget>,
        newCoverPath: String? = null,
        newTitle: String? = null,
    ) {
        operations.runIfIdle {
            entries.editGameOverride(
                gameItem,
                GameOverrideChanges(newTargets, newCoverPath, newTitle),
            )
        }
    }

    /** Updates the app status. */
    fun setStatus(status: AppStatus) {
        library.publishStatus(status)
    }

    // ── Steam Shortcut Management ────────────────────────────────

    /** Checks if Steam's userdata directory is available on this machine. */
    fun isSteamAvailable(): Boolean = steamManager.findShortcutsFile() != null

    /** Checks if a [GameEntry] is already registered as a Steam shortcut. */
    fun isSteamRegistered(entry: GameEntry): Boolean = steamManager.isRegistered(entry)

    /** Registers a [GameEntry] as a Non-Steam Game shortcut in Steam. */
    suspend fun registerInSteam(entry: GameEntry): String =
        operations.runIfIdle {
            recoverUiFailure(
                action = {
                    val shortcut = steamManager.registerEntry(entry)

                    val updatedEntry =
                        when (entry) {
                            is EmulatorSystem -> entry.copy(steamAppId = shortcut.appId)
                            is NativePCGame -> entry.copy(steamAppId = shortcut.appId)
                        }
                    entries.editEntry(entry, updatedEntry)

                    "\"${entry.name}\" registered in Steam! Restart Steam to see it in your library."
                },
                recover = { e ->
                    "Failed to register in Steam: ${e.message}"
                },
            )
        } ?: "Wait for the current game or synchronization to finish."

    /** Removes a [GameEntry] from Steam shortcuts and clears its steamAppId. */
    suspend fun unregisterFromSteam(entry: GameEntry) {
        operations.runIfIdle { unregisterFromSteamInternal(entry) }
    }

    private suspend fun unregisterFromSteamInternal(entry: GameEntry) {
        recoverUiFailure(
            action = {
                steamManager.unregisterEntry(entry)

                val updatedEntry =
                    when (entry) {
                        is EmulatorSystem -> entry.copy(steamAppId = null)
                        is NativePCGame -> entry.copy(steamAppId = null)
                    }
                entries.editEntry(entry, updatedEntry)
            },
            recover = { e ->
                _uiState.update {
                    it.copy(
                        status = AppStatus.Error("Failed to unregister from Steam: ${e.message}")
                    )
                }
            },
        )
    }

    val configFile: File
        get() = configManager.configFile

    // ── Google Drive Authentication ──────────────────────────────

    /** Saves OAuth credentials and optionally starts interactive authorization. */
    suspend fun setupGoogleDrive(
        clientId: String,
        clientSecret: String,
        connectNow: Boolean = true,
    ): String = connection.setup(clientId, clientSecret, connectNow)

    /** Clears the persisted refresh token and all visible linked-account sync indicators. */
    fun unlinkGoogleDrive(): String = connection.unlink()

    /** Authorizes the current connection revision and refreshes the selected entry's status. */
    suspend fun loginGoogleDrive(): String = connection.login()

    // ── In-App Updates ───────────────────────────────────────────

    suspend fun checkForUpdates(manual: Boolean = false) = updates.checkForUpdates(manual)

    fun dismissUpdateDialog() {
        _uiState.update { it.copy(showUpdateDialog = false) }
    }

    fun showUpdateDialog() {
        _uiState.update { it.copy(showUpdateDialog = true) }
    }

    suspend fun downloadAndApplyUpdate(info: UpdateInfo) = updates.download(info)

    fun restartApp(file: File): Boolean = updates.restart(file)

    fun clearStatus() {
        if (operations.isBusy) return
        _uiState.update { it.copy(status = AppStatus.Idle) }
    }

    override fun close() {
        httpClient.close()
        updateManager.close()
    }
}
