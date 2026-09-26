package com.emusync.model

import com.emusync.config.ConfigManager
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertNull

class GameEntryGroupTest {

    @Test
    fun `effectiveCloudFolder defaults to name when group and cloudFolder are null`() {
        val emu = EmulatorSystem(
            name = "Yuzu",
            executablePath = "/usr/bin/yuzu",
            romsDirectory = "/roms/switch",
            extensions = listOf("nsp", "xci"),
        )
        assertEquals("Yuzu", emu.effectiveCloudFolder)
        assertNull(emu.group)
        assertNull(emu.cloudFolder)
    }

    @Test
    fun `effectiveCloudFolder uses group when group is set and cloudFolder is null`() {
        val eden = EmulatorSystem(
            name = "Eden",
            executablePath = "/usr/bin/eden",
            romsDirectory = "/roms/switch",
            extensions = listOf("nsp", "xci"),
            group = "Switch",
        )
        assertEquals("Switch", eden.effectiveCloudFolder)
        assertEquals("Switch", eden.group)
        assertNull(eden.cloudFolder)
    }

    @Test
    fun `effectiveCloudFolder uses cloudFolder over group when both are set`() {
        val yuzu = EmulatorSystem(
            name = "Yuzu Early Access",
            executablePath = "/usr/bin/yuzu-ea",
            romsDirectory = "/roms/switch",
            extensions = listOf("nsp", "xci"),
            group = "Switch (Experimental)",
            cloudFolder = "Switch",
        )
        assertEquals("Switch", yuzu.effectiveCloudFolder)
    }

    @Test
    fun `effectiveCloudFolder trims whitespace and falls back appropriately`() {
        val emu = EmulatorSystem(
            name = "Citra",
            executablePath = "/usr/bin/citra",
            romsDirectory = "/roms/3ds",
            extensions = listOf("3ds"),
            group = "   ",
            cloudFolder = "   ",
        )
        assertEquals("Citra", emu.effectiveCloudFolder)

        val emuWithPaddedGroup = emu.copy(group = "  Nintendo 3DS  ", cloudFolder = "   ")
        assertEquals("Nintendo 3DS", emuWithPaddedGroup.effectiveCloudFolder)
    }

    @Test
    fun `multiple emulators with same group share the exact same effectiveCloudFolder`() {
        val yuzu = EmulatorSystem(
            name = "Yuzu",
            executablePath = "/bin/yuzu",
            romsDirectory = "/roms/switch",
            extensions = listOf("nsp"),
            group = "Switch",
        )
        val eden = EmulatorSystem(
            name = "Eden",
            executablePath = "/bin/eden",
            romsDirectory = "/roms/switch",
            extensions = listOf("nsp"),
            group = "Switch",
        )
        val ryujinx = EmulatorSystem(
            name = "Ryujinx",
            executablePath = "/bin/ryujinx",
            romsDirectory = "/roms/switch",
            extensions = listOf("nsp"),
            group = "Switch",
        )

        assertEquals("Switch", yuzu.effectiveCloudFolder)
        assertEquals("Switch", eden.effectiveCloudFolder)
        assertEquals("Switch", ryujinx.effectiveCloudFolder)
        assertEquals(yuzu.effectiveCloudFolder, eden.effectiveCloudFolder)
        assertEquals(eden.effectiveCloudFolder, ryujinx.effectiveCloudFolder)
    }

    @Test
    fun `ConfigManager serializes and deserializes group and cloudFolder`(@TempDir tempDir: File) {
        val configFile = File(tempDir, "config.json")
        val manager = ConfigManager(configFile.absolutePath)

        val yuzu = EmulatorSystem(
            name = "Yuzu",
            executablePath = "/bin/yuzu",
            romsDirectory = "/roms/switch",
            extensions = listOf("nsp"),
            group = "Switch",
            cloudFolder = "NintendoSwitchSaves",
        )
        val nativeGame = NativePCGame(
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
    fun `ConfigManager deserializes legacy JSON without group or cloudFolder`(@TempDir tempDir: File) {
        val configFile = File(tempDir, "legacy_config.json")
        val legacyJson = """
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
        """.trimIndent()
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
