package com.emusync.steam

import com.emusync.model.EmulatorSystem
import com.emusync.model.NativePCGame
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

class SteamShortcutManagerTest {

    private fun createTestManager(testDir: File, existing: List<SteamShortcut> = emptyList()): SteamShortcutManager {
        val vdfFile = File(testDir, "config/shortcuts.vdf")
        vdfFile.parentFile?.mkdirs()
        BinaryVdf.write(vdfFile, existing)

        return object : SteamShortcutManager() {
            override fun findShortcutsFile(): File = vdfFile
        }
    }

    @Test
    fun `should register emulator entry and create shortcut`(@TempDir tempDir: File) {
        val manager = createTestManager(tempDir)

        val entry = EmulatorSystem(
            name = "Game Boy Advance",
            executablePath = "/usr/bin/mgba-qt",
            romsDirectory = "/roms/gba",
            extensions = listOf("gba"),
        )

        val shortcut = manager.registerEntry(entry)
        assertNotNull(shortcut)
        assertEquals("[EmuSync] Game Boy Advance", shortcut.appName)

        val loaded = manager.loadShortcuts()
        assertEquals(1, loaded.size)
        assertEquals(shortcut.appId, loaded[0].appId)
    }

    @Test
    fun `should preserve appId when re-registering entry with modified path`(@TempDir tempDir: File) {
        val manager = createTestManager(tempDir)

        val originalEntry = EmulatorSystem(
            name = "Playstation 1",
            executablePath = "/old/path/duckstation.AppImage",
            romsDirectory = "/roms/ps1",
            extensions = listOf("bin"),
        )

        val firstShortcut = manager.registerEntry(originalEntry)

        val updatedEntry = originalEntry.copy(
            executablePath = "/new/path/duckstation.AppImage",
            steamAppId = firstShortcut.appId,
        )

        val secondShortcut = manager.registerEntry(updatedEntry)

        assertEquals(firstShortcut.appId, secondShortcut.appId)

        val all = manager.loadShortcuts()
        assertEquals(1, all.size, "Should not create duplicates in shortcuts.vdf")
    }

    @Test
    fun `should unregister entry cleanly`(@TempDir tempDir: File) {
        val manager = createTestManager(tempDir)

        val ps1 = EmulatorSystem(name = "PS1", executablePath = "/duck", romsDirectory = "/roms", extensions = listOf("bin"))
        val ps2 = EmulatorSystem(name = "PS2", executablePath = "/pcsx2", romsDirectory = "/roms2", extensions = listOf("iso"))

        val sc1 = manager.registerEntry(ps1)
        val sc2 = manager.registerEntry(ps2)

        assertEquals(2, manager.loadShortcuts().size)

        manager.unregisterEntry(sc1.appId)
        val remaining = manager.loadShortcuts()

        assertEquals(1, remaining.size)
        assertEquals(sc2.appId, remaining[0].appId)
    }

    @Test
    fun `should register native PC game directly with launch options`(@TempDir tempDir: File) {
        val manager = createTestManager(tempDir)

        val nativeGame = NativePCGame(
            name = "Spelunky",
            executablePath = "/opt/games/spelunky/spelunky",
            arguments = listOf("--fullscreen"),
            savePaths = listOf("/saves/spelunky.sav"),
        )

        val shortcut = manager.registerEntry(nativeGame)
        assertEquals("[EmuSync] Spelunky", shortcut.appName)
        assertEquals("\"/opt/games/spelunky/spelunky\"", shortcut.exe)
        assertTrue(shortcut.launchOptions.contains("%command% --fullscreen"))
    }
}
