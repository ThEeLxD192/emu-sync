package com.emusync.composition

import com.emusync.application.connection.CloudConnection
import com.emusync.config.ConfigManager
import com.emusync.desktop.DesktopCoverFiles
import com.emusync.desktop.openConfigDirectory
import com.emusync.desktop.picker.FilePicker
import com.emusync.domain.config.CloudAuthorization
import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.sync.SaveSynchronizerFactory
import com.emusync.drive.DriveClientFactory
import com.emusync.drive.OAuthFlow
import com.emusync.local.saves.LocalSaveTargets
import com.emusync.runner.DesktopBrowserLauncher
import com.emusync.runner.GameRunner
import com.emusync.scanner.scanRoms
import com.emusync.steam.DesktopSteamLibrary
import com.emusync.steam.SteamShortcutManager
import com.emusync.ui.AppViewModel
import com.emusync.ui.library.GameItem
import com.emusync.ui.library.LibraryServices
import com.emusync.ui.library.form.LibraryFormActions
import com.emusync.ui.updates.UpdateServices
import io.ktor.client.HttpClient
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Owns desktop resources for one application window. Presentation borrows its capabilities. */
internal class DesktopApplication(
    private val store: ConfigManager = ConfigManager(),
    private val client: HttpClient = DriveClientFactory.create(),
    private val updates: DesktopUpdates = composeDesktopUpdates(),
    ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
) : AutoCloseable {
    val viewModel = composeAppViewModel(store, client, updates)
    val configPath: String = store.configFile.absolutePath
    val formActions =
        LibraryFormActions(
            LocalSaveTargets(),
            { label, initial, type ->
                withContext(ioDispatcher) { FilePicker.pickPath(label, initial, type) }
            },
            SteamShortcutManager::detectFullscreenArgs,
            DesktopCoverFiles(),
        )

    fun openConfigDirectory() = openConfigDirectory(store.configFile)

    override fun close() {
        client.close()
        updates.close()
    }
}

/**
 * Composes presentation with borrowed clients; their owner remains responsible for closing them.
 */
internal fun composeAppViewModel(
    configManager: ConfigManager,
    httpClient: HttpClient,
    updates: DesktopUpdates,
    steamManager: SteamShortcutManager = SteamShortcutManager(),
    gameRunner: GameRunner = composeGameRunner(steamManager),
    oauthFlow: OAuthFlow =
        OAuthFlow(httpClient, credentialStore = configManager, browser = DesktopBrowserLauncher()),
    loadGames: suspend (GameEntry) -> List<GameItem> = ::loadLibraryGames,
): AppViewModel =
    AppViewModel(
        LibraryServices(
            configManager,
            DesktopSteamLibrary(steamManager),
            SaveSynchronizerFactory { config, onStatus ->
                composeSaveSynchronizer(httpClient, config, configManager, onStatus)
            },
            { gameRunner.openEmulator(it) },
            loadGames,
            LocalSaveTargets(),
        ),
        CloudConnection(
            configManager,
            CloudAuthorization { config, interactive -> oauthFlow.authorize(config, interactive) },
        ),
        UpdateServices(updates.source, updates.files, updates.installer),
    )

internal suspend fun loadLibraryGames(entry: GameEntry): List<GameItem> =
    when (entry) {
        is EmulatorSystem ->
            scanRoms(entry.romsDirectory, entry.extensions).map {
                GameItem(it.name, entry, it.file)
            }
        is NativePCGame -> listOf(GameItem(entry.name, entry))
    }
