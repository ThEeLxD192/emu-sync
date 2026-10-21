package com.emusync.config

import com.emusync.model.AppConfig
import com.emusync.model.EmulatorSystem
import com.emusync.model.GoogleDriveConfig
import com.emusync.model.NativePCGame
import com.emusync.model.SaveTarget
import com.emusync.model.SaveTargetType
import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.assertThrows
import org.junit.jupiter.api.io.TempDir
import java.io.File
import java.io.IOException
import java.nio.file.Files
import java.nio.file.attribute.PosixFilePermission
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
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
                    coverPathByRom = mapOf(
                        "game.bin" to "/covers/game_custom.png"
                    ),
                    steamAppId = 12345
                ),
                NativePCGame(
                    name = "Hollow Knight",
                    executablePath = "/games/hollow_knight/start.sh",
                    arguments = listOf("--fullscreen"),
                    savePaths = listOf("/saves/hollow_knight.dat"),
                    coverPath = "/covers/hollow_knight.jpg",
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
        assertEquals("/covers/game_custom.png", emu.coverPathByRom["game.bin"])

        val native = reloaded.entries[1] as NativePCGame
        assertEquals("Hollow Knight", native.name)
        assertEquals(listOf("/saves/hollow_knight.dat"), native.savePaths)
        assertEquals("/covers/hollow_knight.jpg", native.coverPath)
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

    @Test
    fun `legacy paths are migrated once to explicit save target types`(@TempDir tempDir: File) {
        val extensionlessFile = File(tempDir, "SAVE_SLOT").apply { writeText("save") }
        val dottedDirectory = File(tempDir, "profile.d").apply { mkdirs() }
        val configFile = File(tempDir, "config.json").apply {
            writeText(
                """
                {
                  "entries": [{
                    "type": "native",
                    "name": "Legacy",
                    "executablePath": "/game/start",
                    "savePaths": [${Json.encodeToString(extensionlessFile.absolutePath)}, ${Json.encodeToString(dottedDirectory.absolutePath)}]
                  }]
                }
                """.trimIndent()
            )
        }

        val migrated = ConfigManager(configFile).load().entries.single() as NativePCGame

        assertEquals(
            listOf(
                SaveTarget(extensionlessFile.absolutePath, SaveTargetType.FILE),
                SaveTarget(dottedDirectory.absolutePath, SaveTargetType.DIRECTORY),
            ),
            migrated.saveTargets,
        )
        assertTrue(configFile.readText().contains("saveTargets"))
    }

    @Test
    fun `explicit save target type is never inferred again`(@TempDir tempDir: File) {
        val ambiguous = File(tempDir, "future.folder")
        val manager = ConfigManager(File(tempDir, "config.json"))
        manager.save(
            AppConfig(entries = listOf(
                NativePCGame(
                    name = "Explicit",
                    executablePath = "/game/start",
                    saveTargets = listOf(SaveTarget(ambiguous.absolutePath, SaveTargetType.DIRECTORY)),
                )
            ))
        )

        val loaded = manager.load().entries.single() as NativePCGame
        assertEquals(SaveTargetType.DIRECTORY, loaded.saveTargets.single().type)
    }

    @Test
    fun `missing dotted legacy path stays unknown until created as a directory`(@TempDir tempDir: File) {
        val dottedPath = File(tempDir, "profile.d")
        val configFile = File(tempDir, "config.json").apply {
            writeText(
                """
                {
                  "entries": [{
                    "type": "native",
                    "name": "Legacy",
                    "executablePath": "/game/start",
                    "savePaths": [${Json.encodeToString(dottedPath.absolutePath)}]
                  }]
                }
                """.trimIndent()
            )
        }
        val manager = ConfigManager(configFile)

        val firstLoad = manager.load().entries.single() as NativePCGame
        assertEquals(SaveTargetType.UNKNOWN, firstLoad.saveTargets.single().type)

        dottedPath.mkdirs()
        File(dottedPath, "save.dat").writeText("save")

        val secondLoad = manager.load().entries.single() as NativePCGame
        assertEquals(SaveTargetType.DIRECTORY, secondLoad.saveTargets.single().type)
        assertTrue(configFile.readText().contains("\"directory\""))
    }

    @Test
    fun `missing legacy per ROM paths migrate and resolve after creation`(@TempDir tempDir: File) {
        val rom = File(tempDir, "Game.rom")
        val dottedDirectory = File(tempDir, "profile.d")
        val configFile = File(tempDir, "config.json")
        val manager = ConfigManager(configFile)
        manager.save(
            AppConfig(entries = listOf(
                EmulatorSystem(
                    name = "Legacy emulator",
                    executablePath = "/emulator",
                    romsDirectory = tempDir.absolutePath,
                    extensions = listOf("rom"),
                    savePathsByRom = mapOf(rom.name to listOf(dottedDirectory.absolutePath)),
                ),
            )),
        )

        val firstLoad = manager.load().entries.single() as EmulatorSystem
        assertEquals(
            SaveTargetType.UNKNOWN,
            firstLoad.saveTargetsByRom[rom.name]?.single()?.type,
        )

        dottedDirectory.mkdirs()
        File(dottedDirectory, "save.dat").writeText("save")

        val secondLoad = manager.load().entries.single() as EmulatorSystem
        assertEquals(
            SaveTargetType.DIRECTORY,
            secondLoad.saveTargetsByRom[rom.name]?.single()?.type,
        )
        assertTrue(configFile.readText().contains("\"directory\""))
    }

    @Test
    fun `interrupted save preserves last valid configuration`(@TempDir tempDir: File) {
        val configFile = File(tempDir, "config.json")
        val original = AppConfig(entries = listOf(NativePCGame("Original", "/game/start", savePaths = listOf("/save.dat"))))
        ConfigManager(configFile).save(original)
        val interruptedManager = ConfigManager(configFile) { throw IOException("simulated power loss") }

        assertFailsWith<IOException> {
            interruptedManager.save(AppConfig(entries = emptyList()))
        }

        assertEquals("Original", (ConfigManager(configFile).load().entries.single() as NativePCGame).name)
        assertEquals(listOf("config.json"), tempDir.list()?.toList())
    }

    @Test
    fun `saved configuration is private on POSIX systems`(@TempDir tempDir: File) {
        val configFile = File(tempDir, "config.json")
        ConfigManager(configFile).save(AppConfig())

        if (configFile.toPath().fileSystem.supportedFileAttributeViews().contains("posix")) {
            assertEquals(
                setOf(PosixFilePermission.OWNER_READ, PosixFilePermission.OWNER_WRITE),
                Files.getPosixFilePermissions(configFile.toPath()),
            )
        }
    }

    @Test
    fun `existing directory target misconfigured as FILE resolves to DIRECTORY on load`(@TempDir tempDir: File) {
        val saveDir = File(tempDir, "CUSA13795").apply { mkdirs() }
        File(saveDir, "save.dat").writeText("data")
        val configFile = File(tempDir, "config.json").apply {
            writeText(
                """
                {
                  "entries": [{
                    "type": "native",
                    "name": "Crash Team Racing",
                    "executablePath": "/game/start",
                    "savePaths": [${Json.encodeToString(saveDir.absolutePath)}],
                    "saveTargets": [{
                      "path": ${Json.encodeToString(saveDir.absolutePath)},
                      "type": "file"
                    }]
                  }]
                }
                """.trimIndent()
            )
        }
        val manager = ConfigManager(configFile)
        val loaded = manager.load().entries.single() as NativePCGame

        assertEquals(SaveTargetType.DIRECTORY, loaded.saveTargets.single().type)
        assertTrue(configFile.readText().contains("\"directory\""))
    }
}
