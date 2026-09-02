package com.emusync.config

import com.emusync.model.AppConfig
import com.emusync.model.EmulatorSystem
import com.emusync.model.GoogleDriveConfig
import com.emusync.model.NativePCGame
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.assertThrows
import org.junit.jupiter.api.io.TempDir
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class ConfigManagerTest {

    @Test
    fun `should serialize and deserialize polymorphic entries correctly`(@TempDir tempDir: File) {
        val configFile = File(tempDir, "config.json")
        val manager = ConfigManager(configFile.absolutePath)

        val sampleConfig = AppConfig(
            googleDrive = GoogleDriveConfig(
                clientId = "test-client-id",
                clientSecret = "test-secret",
                refreshToken = "test-refresh-token",
            ),
            entries = listOf(
                EmulatorSystem(
                    name = "Playstation 1",
                    executablePath = "/usr/bin/duckstation",
                    romsDirectory = "/roms/ps1",
                    extensions = listOf("bin", "cue"),
                    savePathsByRom = mapOf(
                        "game.bin" to listOf("/saves/game_1.mcd")
                    ),
                    steamAppId = 12345
                ),
                NativePCGame(
                    name = "Hollow Knight",
                    executablePath = "/games/hollow_knight/start.sh",
                    arguments = listOf("--fullscreen"),
                    savePaths = listOf("/saves/hollow_knight.dat"),
                    steamAppId = 67890
                )
            )
        )

        manager.save(sampleConfig)
        assertTrue(configFile.exists())

        val reloaded = manager.load()
        assertEquals("test-client-id", reloaded.googleDrive?.clientId)
        assertEquals(2, reloaded.entries.size)

        val emu = reloaded.entries[0] as EmulatorSystem
        assertEquals("Playstation 1", emu.name)
        assertEquals("/usr/bin/duckstation", emu.executablePath)
        assertEquals(listOf("bin", "cue"), emu.extensions)
        assertEquals(12345, emu.steamAppId)
        assertEquals(listOf("/saves/game_1.mcd"), emu.savePathsByRom["game.bin"])

        val native = reloaded.entries[1] as NativePCGame
        assertEquals("Hollow Knight", native.name)
        assertEquals(listOf("/saves/hollow_knight.dat"), native.savePaths)
        assertEquals(67890, native.steamAppId)
    }

    @Test
    fun `should throw IllegalArgumentException when configuration file does not exist`(@TempDir tempDir: File) {
        val nonExistent = File(tempDir, "does_not_exist.json")
        val manager = ConfigManager(nonExistent.absolutePath)

        val exception = assertThrows<IllegalArgumentException> {
            manager.load()
        }
        assertTrue(exception.message!!.contains("Configuration file not found"))
    }
}
