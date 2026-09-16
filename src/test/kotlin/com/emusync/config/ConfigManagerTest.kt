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
    fun `should automatically create default config when file does not exist`(@TempDir tempDir: File) {
        val nonExistent = File(tempDir, "auto_created.json")
        val manager = ConfigManager(nonExistent.absolutePath)

        val loaded = manager.load()
        assertTrue(nonExistent.exists())
        assertEquals(emptyList(), loaded.entries)
        assertEquals(null, loaded.googleDrive)

        // Loading again loads the persisted file
        val reloaded = manager.load()
        assertEquals(emptyList(), reloaded.entries)
    }

    @Test
    fun `should throw IllegalArgumentException when autoCreate is false and file does not exist`(@TempDir tempDir: File) {
        val nonExistent = File(tempDir, "does_not_exist.json")
        val manager = ConfigManager(nonExistent.absolutePath)

        val exception = assertThrows<IllegalArgumentException> {
            manager.load(autoCreate = false)
        }
        assertTrue(exception.message!!.contains("Configuration file not found"))
    }

    @Test
    fun `should create parent directories on save when they do not exist`(@TempDir tempDir: File) {
        val nestedFile = File(tempDir, "sub/deep/folder/config.json")
        val manager = ConfigManager(nestedFile.absolutePath)

        val config = ConfigManager.createDefaultConfig()
        manager.save(config)

        assertTrue(nestedFile.exists())
    }

    @Test
    fun `should resolve explicit custom path over environment or defaults`(@TempDir tempDir: File) {
        val customFile = File(tempDir, "my_custom_config.json")
        val resolved = ConfigManager.resolveDefaultConfigFile(
            customPath = customFile.absolutePath,
            envProvider = { "some_env_val" },
            propertyProvider = { "some_prop_val" },
        )
        assertEquals(customFile.absolutePath, resolved.absolutePath)
    }

    @Test
    fun `should resolve co-located config next to AppImage when it exists`(@TempDir tempDir: File) {
        val appImageFile = File(tempDir, "EmuSync-x86_64.AppImage").apply { createNewFile() }
        val coLocatedConfig = File(tempDir, "config.json").apply { writeText("{}") }

        val resolved = ConfigManager.resolveDefaultConfigFile(
            envProvider = { if (it == "APPIMAGE") appImageFile.absolutePath else null },
            propertyProvider = { null },
            workingDir = File("/nonexistent/cwd"),
        )
        assertEquals(coLocatedConfig.absolutePath, resolved.absolutePath)
    }

    @Test
    fun `should resolve co-located config in working directory when it exists`(@TempDir tempDir: File) {
        val cwdConfig = File(tempDir, "config.json").apply { writeText("{}") }

        val resolved = ConfigManager.resolveDefaultConfigFile(
            envProvider = { null },
            propertyProvider = { null },
            workingDir = tempDir,
        )
        assertEquals(cwdConfig.absolutePath, resolved.absolutePath)
    }

    @Test
    fun `should resolve Windows platform default config path`() {
        val resolved = ConfigManager.getDefaultPlatformConfigFile(
            envProvider = { if (it == "APPDATA") "C:\\Users\\Gamer\\AppData\\Roaming" else null },
            propertyProvider = { if (it == "os.name") "Windows 11" else null },
        )
        assertTrue(resolved.path.contains("AppData") && resolved.path.contains("emusync"))
        assertTrue(resolved.name == "config.json")
    }

    @Test
    fun `should resolve Linux platform default config path`() {
        val resolved = ConfigManager.getDefaultPlatformConfigFile(
            envProvider = { if (it == "XDG_CONFIG_HOME") "/home/gamer/.config" else null },
            propertyProvider = { if (it == "os.name") "Linux" else null },
        )
        assertEquals(File("/home/gamer/.config/emusync/config.json").absolutePath, resolved.absolutePath)
    }
}
