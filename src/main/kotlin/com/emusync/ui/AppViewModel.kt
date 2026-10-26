package com.emusync.ui

import com.emusync.config.ConfigManager
import com.emusync.drive.DriveClientFactory
import com.emusync.drive.OAuthFlow
import com.emusync.drive.SyncOrchestrator
import com.emusync.drive.SaveTargetMappingException
import com.emusync.model.EmulatorSystem
import com.emusync.model.AppConfig
import com.emusync.model.GameEntry
import com.emusync.model.GoogleDriveConfig
import com.emusync.model.NativePCGame
import com.emusync.model.SaveTarget
import com.emusync.model.effectiveSaveTargets
import com.emusync.model.effectiveCloudFolder
import com.emusync.runner.GameRunner
import com.emusync.steam.SteamShortcutManager
import com.emusync.update.UpdateInfo
import com.emusync.update.UpdateManager
import io.ktor.client.HttpClient
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import java.io.File

/**
 * ViewModel / presentation state holder for the EmuSync app.
 *
 * Implements the Single Source of Truth (UDF) pattern via [AppUiState].
 */
class AppViewModel(
    private val configManager: ConfigManager,
    private val steamManager: SteamShortcutManager = SteamShortcutManager(),
    private val httpClient: HttpClient = DriveClientFactory.create(),
    private val updateManager: UpdateManager = UpdateManager(),
    private val gameRunner: GameRunner = GameRunner(),
    private val oauthFlow: OAuthFlow = OAuthFlow(httpClient),
    private val loadGames: suspend (GameEntry) -> List<GameItem> = ::loadLibraryGames,
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
    private val library = LibraryController(_uiState, configManager, httpClient, operations, loadGames,
        ::refreshStoredConnection) { item, result ->
        if (!editGameOverrideInternal(item, result.targets, result.coverPath, result.title)) {
            throw SaveTargetMappingException("Could not apply the save setup. Resolve the configuration error before playing.")
        }
    }

    /**
     * Loads the config.json and populates the initial state.
     * Auto-syncs steamAppId with Steam's shortcuts.vdf to ensure 100% consistency.
     */
    suspend fun loadConfig() {
        if (operations.isBusy) return
        try {
            _uiState.update { it.copy(status = AppStatus.Loading) }
            val cfg = configManager.load()

            val steamShortcuts = steamManager.migrateLegacyLaunchers(cfg.entries)
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
                configManager.update { it.copy(entries = synchronizedEntries) }
            } else {
                cfg
            }

            _uiState.update { it.copy(config = finalConfig, status = AppStatus.Idle) }

            // Auto-select the first entry
            finalConfig.entries.firstOrNull()?.let { selectEntry(it) }
        } catch (e: Exception) {
            if (e is CancellationException) throw e
            _uiState.update { it.copy(status = AppStatus.Error("Failed to load config: ${e.message}")) }
        }
    }

    suspend fun selectEntry(entry: GameEntry) = library.selectEntry(entry)

    suspend fun checkSyncForEntry(entry: GameEntry) = library.checkSyncForEntry(entry)

    suspend fun syncEntryNow(entry: GameEntry) = library.syncEntryNow(entry)

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

        val updatedConfig = configManager.update { current ->
            val byName = current.entries.associateBy { it.name }
            val ordered = currentEntries.mapNotNull { byName[it.name] }
            current.copy(entries = ordered + current.entries.filter { candidate -> ordered.none { it.name == candidate.name } })
        }
        _uiState.update { it.copy(config = updatedConfig) }
    }

    /**
     * Updates the full order of entries and persists the new configuration.
     */
    suspend fun reorderEntriesList(newEntries: List<GameEntry>) {
        val cfg = _uiState.value.config ?: return
        if (cfg.entries == newEntries) return

        val updatedConfig = configManager.update { current ->
            val byName = current.entries.associateBy { it.name }
            val ordered = newEntries.mapNotNull { byName[it.name] }.distinctBy { it.name }
            current.copy(entries = ordered + current.entries.filter { candidate -> ordered.none { it.name == candidate.name } })
        }
        _uiState.update { it.copy(config = updatedConfig) }
    }

    fun completeSaveSetup(targets: List<SaveTarget>, title: String? = null, coverPath: String? = null,
        request: SaveSetupRequest? = null) = library.completeSaveSetup(targets, title, coverPath, request)

    suspend fun launchGame(item: GameItem) = library.launchGame(item)

    /** Opens an emulator interface directly for configuration, without launching a ROM. */
    suspend fun openEmulator(entry: EmulatorSystem) {
        operations.runIfIdle {
            try {
                gameRunner.openEmulator(entry)
            } catch (e: Exception) {
                if (e is CancellationException) throw e
                _uiState.update { it.copy(status = AppStatus.Error("Could not open ${entry.name}: ${e.message}")) }
            }
        }
    }

    /**
     * Adds a new entry to config.json and selects it.
     */
    suspend fun addEntry(entry: GameEntry) {
        if (_uiState.value.config == null) return
        if (entry.name.isBlank() || _uiState.value.config?.entries?.any { it.name == entry.name } == true) {
            setStatus(AppStatus.Error("Choose a non-empty category name that is not already in use."))
            return
        }
        val updatedConfig = configManager.update { it.copy(entries = it.entries + entry) }
        _uiState.update { it.copy(config = updatedConfig) }
        selectEntry(entry)
    }

    /**
     * Edits an existing entry in config.json.
     */
    suspend fun editEntry(oldEntry: GameEntry, newEntry: GameEntry): Boolean =
        operations.runIfIdle { editEntryInternal(oldEntry, newEntry) } ?: false

    private suspend fun editEntryInternal(oldEntry: GameEntry, newEntry: GameEntry): Boolean {
        val cfg = _uiState.value.config ?: return false
        if (cfg.entries.none { it.name == oldEntry.name }) return false
        if (newEntry.name.isBlank() || cfg.entries.any { it.name != oldEntry.name && it.name == newEntry.name }) {
            setStatus(AppStatus.Error("Choose a non-empty category name that is not already in use."))
            return false
        }
        val otherEntries = cfg.entries.filterNot { it === oldEntry || it.name == oldEntry.name }
        val sourceUsesRoot = oldEntry is EmulatorSystem ||
            (oldEntry is NativePCGame && (oldEntry.group == null || oldEntry.cloudFolder != null))
        // Regrouping a shared root is a display edit; keep its existing save location.
        val editedEntry = if (
            sourceUsesRoot && newEntry.cloudFolder.isNullOrBlank() &&
            oldEntry.effectiveCloudFolder != newEntry.effectiveCloudFolder &&
            otherEntries.any { it.effectiveCloudFolder == oldEntry.effectiveCloudFolder }
        ) {
            when (newEntry) {
                is EmulatorSystem -> newEntry.copy(cloudFolder = oldEntry.effectiveCloudFolder)
                is NativePCGame -> newEntry.copy(cloudFolder = oldEntry.effectiveCloudFolder)
            }
        } else newEntry

        if (cfg.googleDrive != null) {
            val orchestrator = SyncOrchestrator(
                client = httpClient,
                config = cfg,
                configManager = configManager,
                onStatus = ::setStatus,
            )
            if (!orchestrator.renameEntryCloudData(oldEntry, editedEntry)) return false
        }

        // Migration suspends for network I/O; retain unrelated edits made while it was running.
        val updatedConfig = configManager.update { current ->
            current.copy(entries = current.entries.map { if (it.name == oldEntry.name) editedEntry else it })
        }
        _uiState.update { it.copy(config = updatedConfig) }
        if (_uiState.value.selectedEntry === oldEntry || _uiState.value.selectedEntry?.name == oldEntry.name) {
            selectEntry(editedEntry)
        }

        // Regenerate the Steam wrapper script if paths changed
        if (editedEntry.steamAppId != null) {
            try {
                steamManager.registerEntry(editedEntry)
            } catch (failure: Exception) {
                if (failure is CancellationException) throw failure
            }
        }

        return true
    }

    /**
     * Deletes an entry from config.json.
     */
    suspend fun deleteEntry(entry: GameEntry) {
        operations.runIfIdle { deleteEntryInternal(entry) }
    }

    private fun deleteEntryInternal(entry: GameEntry) {
        if (_uiState.value.config == null) return
        val updatedConfig = configManager.update { current -> current.copy(entries = current.entries.filter { it.name != entry.name }) }
        _uiState.update { it.copy(config = updatedConfig) }
        if (_uiState.value.selectedEntry === entry || _uiState.value.selectedEntry?.name == entry.name) {
            library.clearSelection()
        }
    }

    /**
     * Updates the save paths, optional cover art, and custom title for a specific game item.
     */
    suspend fun editGameOverride(
        gameItem: GameItem,
        newTargets: List<SaveTarget>,
        newCoverPath: String? = null,
        newTitle: String? = null,
    ) {
        operations.runIfIdle { editGameOverrideInternal(gameItem, newTargets, newCoverPath, newTitle) }
    }

    private suspend fun editGameOverrideInternal(
        gameItem: GameItem,
        newTargets: List<SaveTarget>,
        newCoverPath: String? = null,
        newTitle: String? = null,
    ): Boolean {
        val latestEntry = _uiState.value.config?.entries?.find { it.name == gameItem.entry.name } ?: return false
        val currentGame = gameItem.copy(entry = latestEntry)
        val oldTitle = currentGame.effectiveTitle
        val newPaths = newTargets.map { it.path }
        val cleanTitle = newTitle?.trim()?.takeIf { it.isNotBlank() }

        val newEntry: GameEntry = when (val entry = currentGame.entry) {
            is EmulatorSystem -> {
                val romName = currentGame.romFile?.name ?: return false
                val newSaveOverrides = entry.savePathsByRom.toMutableMap()
                val newTargetOverrides = entry.saveTargetsByRom.toMutableMap()
                val cleanSavePaths = newPaths.map { it.trim() }.filter { it.isNotBlank() }
                if (cleanSavePaths.isEmpty()) {
                    newSaveOverrides.remove(romName)
                    newTargetOverrides.remove(romName)
                } else {
                    newSaveOverrides[romName] = cleanSavePaths
                    newTargetOverrides[romName] = newTargets.filter { it.path.isNotBlank() }
                }

                val newCoverOverrides = entry.coverPathByRom.toMutableMap()
                if (newCoverPath != null) {
                    val cleanCover = newCoverPath.trim().takeIf { it.isNotBlank() }
                    if (cleanCover != null) {
                        newCoverOverrides[romName] = cleanCover
                    } else {
                        newCoverOverrides.remove(romName)
                    }
                }

                val newTitleOverrides = entry.titleByRom.toMutableMap()
                if (cleanTitle != null) {
                    newTitleOverrides[romName] = cleanTitle
                } else if (newTitle != null) {
                    newTitleOverrides.remove(romName)
                }

                entry.copy(
                    savePathsByRom = newSaveOverrides,
                    saveTargetsByRom = newTargetOverrides,
                    coverPathByRom = newCoverOverrides,
                    titleByRom = newTitleOverrides,
                )
            }
            is NativePCGame -> {
                val cleanCover = if (newCoverPath != null) newCoverPath.trim().takeIf { it.isNotBlank() } else entry.coverPath
                val cleanName = cleanTitle ?: entry.name
                entry.copy(
                    name = cleanName,
                    savePaths = newPaths,
                    saveTargets = newTargets,
                    coverPath = cleanCover,
                )
            }
        }

        val newEffectiveTitle = currentGame.copy(entry = newEntry).effectiveTitle
        // Migrate before persisting the title so a blocked merge keeps the old mapping.
        // Native game renames migrate their complete path in editEntry instead.
        if (newEffectiveTitle != oldTitle && newEntry is EmulatorSystem) {
            val cfg = _uiState.value.config
            if (cfg?.googleDrive != null) {
                val orchestrator = SyncOrchestrator(
                    client = httpClient,
                    config = cfg,
                    configManager = configManager,
                    onStatus = ::setStatus,
                )
                val saveTargets = newTargets.ifEmpty { currentGame.effectiveSaveTargets }
                val romName = currentGame.romFile?.name ?: currentGame.name
                val migrated = orchestrator.renameGameCloudData(
                    entry = newEntry,
                    romName = romName,
                    oldTitle = oldTitle,
                    newTitle = newEffectiveTitle,
                    localSaveTargets = saveTargets,
                )
                if (!migrated) return false
            }
        }
        return editEntryInternal(currentGame.entry, newEntry)
    }

    /**
     * Updates the app status.
     */
    fun setStatus(status: AppStatus) {
        library.publishStatus(status)
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
    suspend fun registerInSteam(entry: GameEntry): String = operations.runIfIdle {
        try {
            val shortcut = steamManager.registerEntry(entry)

            val updatedEntry = when (entry) {
                is EmulatorSystem -> entry.copy(steamAppId = shortcut.appId)
                is NativePCGame -> entry.copy(steamAppId = shortcut.appId)
            }
            editEntryInternal(entry, updatedEntry)

            "\"${entry.name}\" registered in Steam! Restart Steam to see it in your library."
        } catch (e: Exception) {
            if (e is CancellationException) throw e
            "Failed to register in Steam: ${e.message}"
        }
    } ?: "Wait for the current game or synchronization to finish."

    /**
     * Removes a [GameEntry] from Steam shortcuts and clears its steamAppId.
     */
    suspend fun unregisterFromSteam(entry: GameEntry) {
        operations.runIfIdle { unregisterFromSteamInternal(entry) }
    }

    private suspend fun unregisterFromSteamInternal(entry: GameEntry) {
        try {
            steamManager.unregisterEntry(entry)

            val updatedEntry = when (entry) {
                is EmulatorSystem -> entry.copy(steamAppId = null)
                is NativePCGame -> entry.copy(steamAppId = null)
            }
            editEntryInternal(entry, updatedEntry)
        } catch (e: Exception) {
            if (e is CancellationException) throw e
            _uiState.update { it.copy(status = AppStatus.Error("Failed to unregister from Steam: ${e.message}")) }
        }
    }

    val configFile: File
        get() = configManager.configFile

    // ── Google Drive Authentication ──────────────────────────────

    /**
     * Updates or sets Google Drive OAuth credentials in config.json.
     * If [connectNow] is true, immediately triggers the browser OAuth login.
     */
    suspend fun setupGoogleDrive(clientId: String, clientSecret: String, connectNow: Boolean = true): String {
        if (_uiState.value.config == null) return "No config loaded"
        val trimmedId = clientId.trim()
        val trimmedSecret = clientSecret.trim()
        if (trimmedId.isBlank() || trimmedSecret.isBlank()) {
            return "Client ID and Client Secret cannot be blank."
        }

        val updatedConfig = configManager.updateGoogleDrive { current ->
            GoogleDriveConfig(
                clientId = trimmedId,
                clientSecret = trimmedSecret,
                refreshToken = current?.refreshToken?.takeIf { current.clientId == trimmedId && current.clientSecret == trimmedSecret },
            )
        }
        publishConnection(updatedConfig)

        return if (connectNow) {
            loginGoogleDrive()
        } else {
            "Google Drive credentials saved."
        }
    }

    /**
     * Unlinks Google Drive by clearing the refresh token from config.json.
     */
    suspend fun unlinkGoogleDrive(): String {
        val cfg = _uiState.value.config ?: return "No config loaded"
        if (cfg.googleDrive == null) return "Google Drive is not configured."
        val updatedConfig = configManager.updateGoogleDrive { it?.copy(refreshToken = null) }
        publishConnection(updatedConfig)
        return "Google Drive disconnected."
    }

    /**
     * Triggers an interactive browser login to link or re-authenticate Google Drive.
     * Updates the UI state with the fresh credentials.
     */
    suspend fun loginGoogleDrive(): String {
        val cfg = _uiState.value.config ?: return "No config loaded"
        if (cfg.googleDrive == null) return "Missing googleDrive configuration in config.json"

        return try {
            oauthFlow.authorize(cfg, configManager, allowInteractive = true)
            val freshConfig = configManager.load()
            publishConnection(freshConfig)
            _uiState.value.selectedEntry?.let { checkSyncForEntry(it) }
            "Google Drive successfully connected!"
        } catch (e: Exception) {
            if (e is CancellationException) throw e
            refreshStoredConnection()
            "Google Drive login failed: ${e.message}"
        }
    }

    private fun publishConnection(config: AppConfig) {
        val status = if (config.googleDrive?.refreshToken.isNullOrBlank()) CloudSyncStatus.NOT_CONFIGURED else CloudSyncStatus.IDLE
        _uiState.update { it.copy(config = config, entrySyncStatus = config.entries.associate { entry -> entry.name to status }) }
    }

    private fun refreshStoredConnection() {
        val current = configManager.load()
        if (current.googleDrive != _uiState.value.config?.googleDrive) publishConnection(current)
    }

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
