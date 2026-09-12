package com.emusync.ui

import com.emusync.config.ConfigManager
import com.emusync.drive.DriveClientFactory
import com.emusync.drive.SyncOrchestrator
import com.emusync.model.EmulatorSystem
import com.emusync.model.GameEntry
import com.emusync.model.NativePCGame
import com.emusync.scanner.scanRoms
import com.emusync.steam.SteamShortcutManager
import io.ktor.client.HttpClient
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/**
 * ViewModel / presentation state holder for the EmuSync app.
 *
 * Implements the Single Source of Truth (UDF) pattern via [AppUiState].
 */
class AppViewModel(
    private val configManager: ConfigManager,
    private val steamManager: SteamShortcutManager = SteamShortcutManager(),
    private val httpClient: HttpClient = DriveClientFactory.create(),
    private val updateManager: com.emusync.update.UpdateManager = com.emusync.update.UpdateManager(httpClient),
) {
    private val _uiState = MutableStateFlow(AppUiState())
    val uiState: StateFlow<AppUiState> = _uiState.asStateFlow()

    private var saveSetupDeferred: CompletableDeferred<List<String>>? = null

    /**
     * Loads the config.json and populates the initial state.
     * Auto-syncs steamAppId with Steam's shortcuts.vdf to ensure 100% consistency.
     */
    suspend fun loadConfig() {
        try {
            _uiState.update { it.copy(status = AppStatus.Loading) }
            val cfg = configManager.load()

            val steamShortcuts = steamManager.loadShortcuts()
            var configChanged = false
            val synchronizedEntries = cfg.entries.map { entry ->
                val matchingShortcut = steamShortcuts.find { it.appName == "[EmuSync] ${entry.name}" }
                if (matchingShortcut != null && entry.steamAppId != matchingShortcut.appId) {
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

            val finalConfig = if (configChanged) {
                val updated = cfg.copy(entries = synchronizedEntries)
                configManager.save(updated)
                updated
            } else {
                cfg
            }

            _uiState.update { it.copy(config = finalConfig, status = AppStatus.Idle) }

            // Auto-select the first entry
            finalConfig.entries.firstOrNull()?.let { selectEntry(it) }
        } catch (e: Exception) {
            _uiState.update { it.copy(status = AppStatus.Error("Failed to load config: ${e.message}")) }
        }
    }

    /**
     * Selects a category from the sidebar and loads its games.
     */
    suspend fun selectEntry(entry: GameEntry) {
        _uiState.update { it.copy(isLoading = true, selectedEntry = entry) }
        val items = when (entry) {
            is EmulatorSystem -> {
                val roms = scanRoms(entry.romsDirectory, entry.extensions)
                roms.map { rom ->
                    GameItem(
                        name = rom.name,
                        entry = entry,
                        romFile = rom.file,
                    )
                }
            }
            is NativePCGame -> {
                listOf(
                    GameItem(
                        name = entry.name,
                        entry = entry,
                        romFile = null,
                    )
                )
            }
        }
        _uiState.update { it.copy(isLoading = false, gameItems = items) }

        // Trigger background sync check if Google Drive is linked
        if (_uiState.value.config?.googleDrive?.refreshToken?.isNotBlank() == true) {
            checkSyncForEntry(entry)
        }
    }

    /**
     * Checks the cloud sync status of a specific entry without launching any game.
     */
    suspend fun checkSyncForEntry(entry: GameEntry) {
        val cfg = _uiState.value.config ?: return
        if (cfg.googleDrive == null || cfg.googleDrive.refreshToken.isNullOrBlank()) {
            _uiState.update { it.copy(entrySyncStatus = it.entrySyncStatus + (entry.name to CloudSyncStatus.NOT_CONFIGURED)) }
            return
        }

        _uiState.update { it.copy(entrySyncStatus = it.entrySyncStatus + (entry.name to CloudSyncStatus.CHECKING)) }
        val orchestrator = SyncOrchestrator(
            client = httpClient,
            config = cfg,
            configManager = configManager,
            onStatus = { /* silent during check */ },
        )
        val status = orchestrator.checkEntrySyncStatus(entry)
        _uiState.update { it.copy(entrySyncStatus = it.entrySyncStatus + (entry.name to status)) }
    }

    /**
     * Triggers manual synchronization for an entry without launching any game.
     */
    suspend fun syncEntryNow(entry: GameEntry) {
        val cfg = _uiState.value.config ?: return
        _uiState.update { it.copy(entrySyncStatus = it.entrySyncStatus + (entry.name to CloudSyncStatus.SYNCING)) }

        val orchestrator = SyncOrchestrator(
            client = httpClient,
            config = cfg,
            configManager = configManager,
            onStatus = { setStatus(it) },
        )

        val success = orchestrator.syncEntry(entry)
        if (success) {
            _uiState.update { it.copy(entrySyncStatus = it.entrySyncStatus + (entry.name to CloudSyncStatus.IN_SYNC)) }
        } else {
            checkSyncForEntry(entry)
        }
    }

    /**
     * Reorders entries in the list and persists the new order to config.json.
     */
    suspend fun reorderEntries(fromIndex: Int, toIndex: Int) {
        val cfg = _uiState.value.config ?: return
        val currentEntries = cfg.entries.toMutableList()
        if (fromIndex !in currentEntries.indices || toIndex !in currentEntries.indices || fromIndex == toIndex) {
            return
        }

        val movedItem = currentEntries.removeAt(fromIndex)
        currentEntries.add(toIndex, movedItem)

        val updatedConfig = cfg.copy(entries = currentEntries)
        configManager.save(updatedConfig)
        _uiState.update { it.copy(config = updatedConfig) }
    }

    /**
     * Completes a pending save setup request from the UI.
     */
    fun completeSaveSetup(paths: List<String>) {
        saveSetupDeferred?.complete(paths)
        saveSetupDeferred = null
        _uiState.update { it.copy(saveSetupRequest = null) }
    }

    /**
     * Pauses coroutine execution, asks the UI for save paths, and resumes.
     */
    private suspend fun requestSaveSetup(item: GameItem): List<String> {
        val deferred = CompletableDeferred<List<String>>()
        saveSetupDeferred = deferred
        _uiState.update { it.copy(saveSetupRequest = item) }
        return deferred.await()
    }

    /**
     * Launches a game with the full Pre-Sync → Play → Post-Sync flow.
     */
    suspend fun launchGame(item: GameItem) {
        val cfg = _uiState.value.config ?: return

        val orchestrator = SyncOrchestrator(
            client = httpClient,
            config = cfg,
            configManager = configManager,
            onStatus = { status -> setStatus(status) },
        )

        orchestrator.playWithSync(item, onRequestSaveSetup = { reqItem ->
            val paths = requestSaveSetup(reqItem)
            if (paths.isNotEmpty()) {
                editGameOverride(reqItem, paths)
            }
            paths
        })
        checkSyncForEntry(item.entry)
    }

    /**
     * Adds a new entry to config.json and selects it.
     */
    suspend fun addEntry(entry: GameEntry) {
        val cfg = _uiState.value.config ?: return
        val updatedConfig = cfg.copy(entries = cfg.entries + entry)
        configManager.save(updatedConfig)
        _uiState.update { it.copy(config = updatedConfig) }
        selectEntry(entry)
    }

    /**
     * Edits an existing entry in config.json.
     */
    suspend fun editEntry(oldEntry: GameEntry, newEntry: GameEntry) {
        val cfg = _uiState.value.config ?: return
        val newEntries = cfg.entries.map { if (it === oldEntry || it.name == oldEntry.name) newEntry else it }
        val updatedConfig = cfg.copy(entries = newEntries)
        configManager.save(updatedConfig)
        _uiState.update { it.copy(config = updatedConfig) }
        if (_uiState.value.selectedEntry === oldEntry || _uiState.value.selectedEntry?.name == oldEntry.name) {
            selectEntry(newEntry)
        }

        // Regenerate the Steam wrapper script if paths changed
        if (newEntry is EmulatorSystem && newEntry.steamAppId != null) {
            try {
                steamManager.registerEntry(newEntry)
            } catch (_: Exception) { /* best-effort */ }
        }
    }

    /**
     * Deletes an entry from config.json.
     */
    suspend fun deleteEntry(entry: GameEntry) {
        val cfg = _uiState.value.config ?: return
        val newEntries = cfg.entries.filter { it !== entry && it.name != entry.name }
        val updatedConfig = cfg.copy(entries = newEntries)
        configManager.save(updatedConfig)
        _uiState.update { it.copy(config = updatedConfig) }
        if (_uiState.value.selectedEntry === entry || _uiState.value.selectedEntry?.name == entry.name) {
            _uiState.update { it.copy(selectedEntry = null, gameItems = emptyList()) }
        }
    }

    /**
     * Updates the savePathsByRom logic for a specific ROM within an Emulator System.
     */
    suspend fun editGameOverride(gameItem: GameItem, newPaths: List<String>) {
        val entry = gameItem.entry
        if (entry !is EmulatorSystem) return
        val romName = gameItem.romFile?.name ?: return

        val newOverrides = entry.savePathsByRom.toMutableMap()
        newOverrides[romName] = newPaths

        val newEntry = entry.copy(savePathsByRom = newOverrides)
        editEntry(entry, newEntry)
    }

    /**
     * Updates the app status.
     */
    fun setStatus(status: AppStatus) {
        _uiState.update { it.copy(status = status) }
    }

    // ── Steam Shortcut Management ────────────────────────────────

    /**
     * Checks if Steam's userdata directory is available on this machine.
     */
    fun isSteamAvailable(): Boolean = steamManager.findShortcutsFile() != null

    /**
     * Checks if a [GameEntry] is already registered as a Steam shortcut.
     */
    fun isSteamRegistered(entry: GameEntry): Boolean = steamManager.isRegistered(entry)

    /**
     * Registers a [GameEntry] as a Non-Steam Game shortcut in Steam.
     */
    suspend fun registerInSteam(entry: GameEntry): String {
        return try {
            val shortcut = steamManager.registerEntry(entry)

            val updatedEntry = when (entry) {
                is EmulatorSystem -> entry.copy(steamAppId = shortcut.appId)
                is NativePCGame -> entry.copy(steamAppId = shortcut.appId)
            }
            editEntry(entry, updatedEntry)

            "\"${entry.name}\" registered in Steam! Restart Steam to see it in your library."
        } catch (e: Exception) {
            "Failed to register in Steam: ${e.message}"
        }
    }

    /**
     * Removes a [GameEntry] from Steam shortcuts and clears its steamAppId.
     */
    suspend fun unregisterFromSteam(entry: GameEntry) {
        try {
            steamManager.unregisterEntry(entry)

            val updatedEntry = when (entry) {
                is EmulatorSystem -> entry.copy(steamAppId = null)
                is NativePCGame -> entry.copy(steamAppId = null)
            }
            editEntry(entry, updatedEntry)
        } catch (e: Exception) {
            _uiState.update { it.copy(status = AppStatus.Error("Failed to unregister from Steam: ${e.message}")) }
        }
    }

    // ── Google Drive Authentication ──────────────────────────────

    /**
     * Triggers an interactive browser login to link or re-authenticate Google Drive.
     * Updates the UI state with the fresh credentials.
     */
    suspend fun loginGoogleDrive(): String {
        val cfg = _uiState.value.config ?: return "No config loaded"
        if (cfg.googleDrive == null) return "Missing googleDrive configuration in config.json"

        return try {
            val flow = com.emusync.drive.OAuthFlow(httpClient)
            flow.authorize(cfg, configManager, allowInteractive = true)
            val freshConfig = configManager.load()
            _uiState.update { it.copy(config = freshConfig) }
            "Google Drive successfully connected!"
        } catch (e: Exception) {
            "Google Drive login failed: ${e.message}"
        }
    }

    // ── In-App Updates ───────────────────────────────────────────

    suspend fun checkForUpdates(manual: Boolean = false) {
        _uiState.update { it.copy(updateState = UpdateUiState.Checking) }
        val info = updateManager.checkForUpdates()
        if (info != null) {
            _uiState.update { it.copy(updateState = UpdateUiState.Available(info), showUpdateDialog = manual) }
        } else {
            _uiState.update { it.copy(updateState = UpdateUiState.Idle) }
        }
    }

    fun dismissUpdateDialog() {
        _uiState.update { it.copy(showUpdateDialog = false) }
    }

    fun showUpdateDialog() {
        _uiState.update { it.copy(showUpdateDialog = true) }
    }

    suspend fun downloadAndApplyUpdate(info: com.emusync.update.UpdateInfo) {
        _uiState.update { it.copy(updateState = UpdateUiState.Downloading(0f, info)) }
        val tempFile = java.io.File(System.getProperty("java.io.tmpdir"), "EmuSync-update.AppImage")
        val success = updateManager.downloadUpdate(info.downloadUrl, tempFile) { progress ->
            _uiState.update { it.copy(updateState = UpdateUiState.Downloading(progress, info)) }
        }
        if (success) {
            _uiState.update { it.copy(updateState = UpdateUiState.ReadyToRestart(tempFile)) }
        } else {
            _uiState.update { it.copy(updateState = UpdateUiState.Error("Failed to download update")) }
        }
    }

    fun restartApp(file: java.io.File): Boolean {
        return updateManager.applyUpdateAndRestart(file)
    }
}
