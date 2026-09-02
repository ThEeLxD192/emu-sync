package com.emusync.steam

import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import kotlin.test.assertEquals

class BinaryVdfTest {

    @Test
    fun `should round-trip encode and decode Steam shortcuts binary VDF`(@TempDir tempDir: File) {
        val testFile = File(tempDir, "shortcuts.vdf")

        val originalShortcuts = listOf(
            SteamShortcut(
                appId = -123456789,
                appName = "[EmuSync] Duckstation",
                exe = "\"/usr/bin/duckstation\"",
                startDir = "/home/user",
                launchOptions = "--bigpicture -fullscreen",
                tags = mapOf("0" to "EmuSync", "1" to "Emulator"),
            ),
            SteamShortcut(
                appId = -987654321,
                appName = "Spelunky 2",
                exe = "\"/games/spelunky/spelunky2\"",
                startDir = "/games/spelunky",
            )
        )

        BinaryVdf.write(testFile, originalShortcuts)
        val readBack = BinaryVdf.read(testFile)

        assertEquals(originalShortcuts.size, readBack.size)

        for (i in originalShortcuts.indices) {
            val orig = originalShortcuts[i]
            val copy = readBack[i]
            assertEquals(orig.appId, copy.appId)
            assertEquals(orig.appName, copy.appName)
            assertEquals(orig.exe, copy.exe)
            assertEquals(orig.startDir, copy.startDir)
            assertEquals(orig.launchOptions, copy.launchOptions)
            assertEquals(orig.tags, copy.tags)
        }
    }

    @Test
    fun `should append new shortcut and preserve existing shortcuts`(@TempDir tempDir: File) {
        val testFile = File(tempDir, "shortcuts.vdf")

        val existing = listOf(
            SteamShortcut(
                appId = -111111,
                appName = "Spotify",
                exe = "\"/usr/bin/spotify\"",
                startDir = "/home/user",
            )
        )

        BinaryVdf.write(testFile, existing)

        val newEntry = SteamShortcut(
            appId = -222222,
            appName = "[EmuSync] PCSX2",
            exe = "\"/usr/bin/pcsx2\"",
            startDir = "/home/user",
        )

        BinaryVdf.write(testFile, existing + newEntry)
        val result = BinaryVdf.read(testFile)

        assertEquals(2, result.size)
        assertEquals("Spotify", result[0].appName)
        assertEquals("[EmuSync] PCSX2", result[1].appName)
    }

    @Test
    fun `should handle empty shortcuts list`(@TempDir tempDir: File) {
        val testFile = File(tempDir, "empty_shortcuts.vdf")
        BinaryVdf.write(testFile, emptyList())

        val result = BinaryVdf.read(testFile)
        assertEquals(0, result.size)
    }
}
