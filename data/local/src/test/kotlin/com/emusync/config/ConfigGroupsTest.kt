package com.emusync.config

import com.emusync.domain.config.AppConfig
import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.effectiveCloudFolder
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertNull
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class ConfigGroupsTest {

    @Test
    fun `ConfigManager serializes and deserializes group and cloudFolder`(@TempDir tempDir: File) {
        val configFile = File(tempDir, "config.json")
        val manager = ConfigManager(configFile.absolutePath)

        val yuzu =
            EmulatorSystem(
                name = "Yuzu",
                executablePath = "/bin/yuzu",
                romsDirectory = "/roms/switch",
                extensions = listOf("nsp"),
                group = "Switch",
                cloudFolder = "NintendoSwitchSaves",
            )
        val nativeGame =
            NativePCGame(
                name = "Hollow Knight",
                executablePath = "/games/hk/start.sh",
                group = "Indie Games",
            )

        manager.save(AppConfig(entries = listOf(yuzu, nativeGame)))

        val reloaded = manager.load()
        val loadedYuzu = reloaded.entries[0] as EmulatorSystem
        assertEquals("Switch", loadedYuzu.group)
        assertEquals("NintendoSwitchSaves", loadedYuzu.cloudFolder)
        assertEquals("NintendoSwitchSaves", loadedYuzu.effectiveCloudFolder)

        val loadedNative = reloaded.entries[1] as NativePCGame
        assertEquals("Indie Games", loadedNative.group)
        assertNull(loadedNative.cloudFolder)
        assertEquals("Indie Games", loadedNative.effectiveCloudFolder)
    }

    @Test
    fun `ConfigManager deserializes legacy JSON without group or cloudFolder`(
        @TempDir tempDir: File
    ) {
        val configFile = File(tempDir, "legacy_config.json")
        val legacyJson =
            """
            {
              "entries": [
                {
                  "type": "emulator",
                  "name": "SNES",
                  "executablePath": "/usr/bin/snes9x",
                  "romsDirectory": "/roms/snes",
                  "extensions": ["smc", "sfc"]
                }
              ]
            }
            """
                .trimIndent()
        configFile.writeText(legacyJson)

        val manager = ConfigManager(configFile.absolutePath)
        val loaded = manager.load()
        assertEquals(1, loaded.entries.size)
        val snes = loaded.entries[0] as EmulatorSystem
        assertEquals("SNES", snes.name)
        assertNull(snes.group)
        assertNull(snes.cloudFolder)
        assertEquals("SNES", snes.effectiveCloudFolder)
    }
}
