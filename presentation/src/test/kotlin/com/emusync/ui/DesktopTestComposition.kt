package com.emusync.ui

import com.emusync.application.connection.CloudConnection
import com.emusync.application.library.gameSpecificTargets
import com.emusync.application.library.gameTargets
import com.emusync.config.ConfigManager
import com.emusync.domain.config.AppConfig
import com.emusync.domain.config.CloudAuthorization
import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.sync.SaveSynchronizerFactory
import com.emusync.drive.OAuthFlow
import com.emusync.local.saves.LocalSaveTargets
import com.emusync.runner.DesktopBrowserLauncher
import com.emusync.runner.GameRunner
import com.emusync.scanner.scanRoms
import com.emusync.steam.DesktopSteamLibrary
import com.emusync.steam.SteamShortcutManager
import com.emusync.ui.library.GameItem
import com.emusync.ui.library.LibraryServices
import com.emusync.ui.library.sync.createSaveSynchronizer as createPresentationSynchronizer
import com.emusync.ui.library.sync.playWithSync as playPresentationGame
import com.emusync.ui.updates.UpdateServices
import com.emusync.update.DesktopUpdateInstaller
import com.emusync.update.GitHubUpdateSource
import com.emusync.update.UpdateStagingFiles
import io.ktor.client.HttpClient
import java.io.File
import kotlin.coroutines.ContinuationInterceptor
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.currentCoroutineContext

internal suspend fun createTestViewModel(
    configManager: ConfigManager,
    httpClient: HttpClient,
    steamManager: SteamShortcutManager = SteamShortcutManager(),
    updates: UpdateServices =
        UpdateServices(
            GitHubUpdateSource(httpClient, "owner", "repo", "1.0.0", false),
            UpdateStagingFiles({ false }, { null }),
            DesktopUpdateInstaller(),
        ),
    gameRunner: GameRunner = GameRunner(DesktopSteamLibrary(steamManager)),
    oauthFlow: OAuthFlow =
        OAuthFlow(httpClient, credentialStore = configManager, browser = DesktopBrowserLauncher()),
    loadGames: suspend (GameEntry) -> List<GameItem> = ::loadTestLibraryGames,
    ioDispatcher: CoroutineDispatcher? = null,
) =
    AppViewModel(
        LibraryServices(
            configManager,
            DesktopSteamLibrary(steamManager),
            SaveSynchronizerFactory { config, onStatus ->
                testSaveSynchronizer(httpClient, config, configManager, onStatus)
            },
            { gameRunner.openEmulator(it) },
            loadGames,
            LocalSaveTargets(),
        ),
        CloudConnection(
            configManager,
            CloudAuthorization { config, interactive -> oauthFlow.authorize(config, interactive) },
        ),
        updates,
        ioDispatcher
            ?: checkNotNull(
                currentCoroutineContext()[ContinuationInterceptor] as? CoroutineDispatcher
            ),
    )

internal fun testGameItem(name: String, entry: GameEntry, romFile: File? = null) =
    GameItem(name, entry, romFile)

internal fun createSaveSynchronizer(
    client: HttpClient,
    config: AppConfig,
    configManager: ConfigManager,
    onStatus: (AppStatus) -> Unit,
) =
    createPresentationSynchronizer(
        SaveSynchronizerFactory { current, callback ->
            testSaveSynchronizer(client, current, configManager, callback)
        },
        config,
        onStatus,
    )

internal suspend fun loadTestLibraryGames(entry: GameEntry): List<GameItem> =
    when (entry) {
        is EmulatorSystem ->
            scanRoms(entry.romsDirectory, entry.extensions).map {
                testGameItem(it.name, entry, it.file)
            }
        is NativePCGame -> listOf(testGameItem(entry.name, entry))
    }

internal val GameItem.effectiveSaveTargets
    get() = LocalSaveTargets().gameTargets(entry, romFile?.name)
internal val GameItem.gameSpecificSaveTargets
    get() = LocalSaveTargets().gameSpecificTargets(entry, romFile?.name)
internal val GameItem.effectiveSavePaths
    get() = effectiveSaveTargets.map { it.path }
internal val GameItem.gameSpecificSavePaths
    get() = gameSpecificSaveTargets.map { it.path }

internal suspend fun com.emusync.domain.sync.SaveSynchronizer.playWithSync(
    item: GameItem,
    resolveConfiguredItem: (GameItem) -> GameItem = { it },
    onRequestSaveSetup:
        suspend (GameItem, String?, List<String>) -> List<com.emusync.domain.library.SaveTarget> =
        { _, _, _ ->
            emptyList()
        },
) = playPresentationGame(item, LocalSaveTargets(), resolveConfiguredItem, onRequestSaveSetup)
