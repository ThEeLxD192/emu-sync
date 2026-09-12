package com.emusync.ui

import com.emusync.config.ConfigManager
import com.emusync.model.AppConfig
import com.emusync.model.EmulatorSystem
import com.emusync.model.NativePCGame
import com.emusync.steam.SteamShortcutManager
import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respondOk
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

class AppViewModelTest {

    private fun createMockHttpClient(): HttpClient {
        return HttpClient(MockEngine) {
            engine {
                addHandler { respondOk() }
            }
        }
    }

    @Test
    fun `should load config and auto-select first entry in uiState`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)

        val initialConfig = AppConfig(
            entries = listOf(
                EmulatorSystem(
                    name = "GBA",
                    executablePath = "mgba",
                    romsDirectory = tempDir.absolutePath,
                    extensions = listOf("gba"),
                ),
                NativePCGame(
                    name = "Spelunky",
                    executablePath = "spelunky",
                    savePaths = emptyList(),
                )
            )
        )
        configManager.save(initialConfig)

        // Create dummy ROM for GBA
        File(tempDir, "zelda.gba").createNewFile()

        val viewModel = AppViewModel(
            configManager = configManager,
            steamManager = SteamShortcutManager(),
            httpClient = createMockHttpClient(),
        )

        viewModel.loadConfig()

        val state = viewModel.uiState.value
        assertNotNull(state.config)
        assertEquals(2, state.config!!.entries.size)

        assertNotNull(state.selectedEntry)
        assertEquals("GBA", state.selectedEntry!!.name)

        assertEquals(1, state.gameItems.size)
        assertEquals("zelda", state.gameItems[0].name)
    }

    @Test
    fun `should select native game entry and update uiState`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)

        val nativeGame = NativePCGame(
            name = "Celeste",
            executablePath = "celeste",
            savePaths = listOf("/saves/celeste.sav"),
        )
        configManager.save(AppConfig(entries = listOf(nativeGame)))

        val viewModel = AppViewModel(
            configManager = configManager,
            httpClient = createMockHttpClient(),
        )

        viewModel.loadConfig()

        val state = viewModel.uiState.value
        assertEquals(1, state.gameItems.size)
        assertEquals("Celeste", state.gameItems[0].name)
        assertEquals(listOf("/saves/celeste.sav"), state.gameItems[0].effectiveSavePaths)
    }

    @Test
    fun `should update status reactively in uiState`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)
        configManager.save(AppConfig(entries = emptyList()))

        val viewModel = AppViewModel(
            configManager = configManager,
            httpClient = createMockHttpClient(),
        )

        assertEquals(AppStatus.Idle, viewModel.uiState.value.status)

        viewModel.setStatus(AppStatus.Syncing("Uploading saves..."))
        assertTrue(viewModel.uiState.value.status is AppStatus.Syncing)
        assertEquals("Uploading saves...", (viewModel.uiState.value.status as AppStatus.Syncing).message)

        viewModel.setStatus(AppStatus.Playing)
        assertEquals(AppStatus.Playing, viewModel.uiState.value.status)
    }

    @Test
    fun `reorderEntries should change entry positions and persist to config`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)

        val entry1 = NativePCGame(name = "Play 1", executablePath = "p1")
        val entry2 = NativePCGame(name = "Play 2", executablePath = "p2")
        val entry3 = NativePCGame(name = "Spelunky", executablePath = "spelunky")

        configManager.save(AppConfig(entries = listOf(entry1, entry2, entry3)))

        val viewModel = AppViewModel(
            configManager = configManager,
            httpClient = createMockHttpClient(),
        )
        viewModel.loadConfig()

        // Initially: Play 1 (0), Play 2 (1), Spelunky (2)
        assertEquals(listOf("Play 1", "Play 2", "Spelunky"), viewModel.uiState.value.config!!.entries.map { it.name })

        // Move Spelunky from index 2 to index 0 -> Spelunky, Play 1, Play 2
        viewModel.reorderEntries(fromIndex = 2, toIndex = 0)

        assertEquals(listOf("Spelunky", "Play 1", "Play 2"), viewModel.uiState.value.config!!.entries.map { it.name })

        // Verify persisted to config.json file
        val reloaded = configManager.load()
        assertEquals(listOf("Spelunky", "Play 1", "Play 2"), reloaded.entries.map { it.name })
    }
}
