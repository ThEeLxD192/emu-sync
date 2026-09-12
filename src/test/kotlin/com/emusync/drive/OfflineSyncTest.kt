package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.model.AppConfig
import com.emusync.model.EmulatorSystem
import com.emusync.model.GoogleDriveConfig
import com.emusync.ui.AppStatus
import com.emusync.ui.GameItem
import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respondError
import io.ktor.http.HttpStatusCode
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.assertThrows
import org.junit.jupiter.api.io.TempDir
import java.io.File
import java.io.IOException
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class OfflineSyncTest {

    @Test
    fun `authorize with allowInteractive false should fail fast when no refresh token`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)
        val config = AppConfig(
            googleDrive = GoogleDriveConfig(
                clientId = "test-client",
                clientSecret = "test-secret",
                refreshToken = null
            ),
            entries = emptyList(),
        )
        configManager.save(config)

        val mockClient = HttpClient(MockEngine) {
            engine {
                addHandler { respondError(HttpStatusCode.InternalServerError) }
            }
        }

        val oauthFlow = OAuthFlow(mockClient)

        val exception = assertThrows<DriveApiException> {
            oauthFlow.authorize(config, configManager, allowInteractive = false)
        }
        assertTrue(exception.message!!.contains("not authenticated or offline"))
    }

    @Test
    fun `authorize with allowInteractive false should fail fast on network error without clearing token`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)
        val config = AppConfig(
            googleDrive = GoogleDriveConfig(
                clientId = "test-client",
                clientSecret = "test-secret",
                refreshToken = "existing-valid-refresh-token"
            ),
            entries = emptyList(),
        )
        configManager.save(config)

        val mockClient = HttpClient(MockEngine) {
            engine {
                addHandler { throw IOException("Network unreachable (offline)") }
            }
        }

        val oauthFlow = OAuthFlow(mockClient)

        val exception = assertThrows<DriveApiException> {
            oauthFlow.authorize(config, configManager, allowInteractive = false)
        }
        assertTrue(exception.message!!.contains("offline"))

        // Refresh token should be preserved in config!
        val reloaded = configManager.load()
        assertEquals("existing-valid-refresh-token", reloaded.googleDrive?.refreshToken)
    }

    @Test
    fun `SyncOrchestrator should launch game and return to Idle when offline`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)

        val dummyRom = File(tempDir, "game.bin").apply { writeText("game data") }
        val dummyExecutable = File(tempDir, "emu.sh").apply {
            writeText("#!/bin/sh\nexit 0\n")
            setExecutable(true)
        }

        val system = EmulatorSystem(
            name = "TestSystem",
            executablePath = dummyExecutable.absolutePath,
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("bin"),
            savePaths = listOf(File(tempDir, "save.dat").absolutePath)
        )

        val config = AppConfig(
            googleDrive = GoogleDriveConfig(
                clientId = "test-client",
                clientSecret = "test-secret",
                refreshToken = "valid-refresh-token"
            ),
            entries = listOf(system)
        )
        configManager.save(config)

        // Mock HTTP client throwing offline errors
        val mockClient = HttpClient(MockEngine) {
            engine {
                addHandler { throw IOException("No internet connection") }
            }
        }

        val statuses = mutableListOf<AppStatus>()
        val orchestrator = SyncOrchestrator(
            client = mockClient,
            config = config,
            configManager = configManager,
            onStatus = { statuses.add(it) }
        )

        val gameItem = GameItem(name = "game", entry = system, romFile = dummyRom)

        // Should NOT throw, should complete play flow cleanly
        orchestrator.playWithSync(gameItem)

        // Verifies status transitioned through Playing and ended in Idle
        assertTrue(statuses.any { it is AppStatus.Playing })
        assertEquals(AppStatus.Idle, statuses.last())
    }
}
