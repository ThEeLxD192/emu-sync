package com.emusync.steam

import com.emusync.model.EmulatorSystem
import com.emusync.model.NativePCGame
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

class SteamShortcutManagerTest {

    private fun createTestManager(
        testDir: File,
        existing: List<SteamShortcut> = emptyList(),
        isWindows: Boolean = false,
        registryQuery: (String, String) -> String? = { _, _ -> null },
    ): SteamShortcutManager {
        val vdfFile = File(testDir, "config/shortcuts.vdf")
        vdfFile.parentFile?.mkdirs()
        BinaryVdf.write(vdfFile, existing)

        return object : SteamShortcutManager(
            customLauncherDir = File(testDir, "launchers"),
            registryQuery = registryQuery,
        ) {
            override fun findShortcutsFile(): File = vdfFile
            override fun isWindows(): Boolean = isWindows
        }
    }

    @Test
    fun `should register emulator entry and create shortcut on Linux`(@TempDir tempDir: File) {
        val manager = createTestManager(tempDir, isWindows = false)

        val entry = EmulatorSystem(
            name = "Game Boy Advance",
            executablePath = "/usr/bin/mgba-qt",
            romsDirectory = "/roms/gba",
            extensions = listOf("gba"),
        )

        val shortcut = manager.registerEntry(entry)
        assertNotNull(shortcut)
        assertEquals("[EmuSync] Game Boy Advance", shortcut.appName)
        assertTrue(shortcut.exe.endsWith("launch_game_boy_advance.sh\""))
        assertEquals("", shortcut.launchOptions)

        val loaded = manager.loadShortcuts()
        assertEquals(1, loaded.size)
        assertEquals(shortcut.appId, loaded[0].appId)

        // Verify Linux launcher script
        val launcherDir = File(tempDir, "launchers")
        val script = File(launcherDir, "launch_game_boy_advance.sh")
        assertTrue(script.exists())
        val content = script.readText()
        assertTrue(content.startsWith("#!/bin/bash"))
        assertTrue(content.contains("FULLSCREEN_ARGS=\"-f\""))
        assertTrue(content.contains("exec \"/usr/bin/mgba-qt\" \$FULLSCREEN_ARGS \"\$ROM_PATH\""))
    }

    @Test
    fun `should register emulator entry and create batch script on Windows`(@TempDir tempDir: File) {
        val manager = createTestManager(tempDir, isWindows = true)

        val entry = EmulatorSystem(
            name = "Game Boy Advance",
            executablePath = "C:\\Emulators\\mGBA\\mgba.exe",
            romsDirectory = "C:\\ROMs\\gba",
            extensions = listOf("gba"),
        )

        val shortcut = manager.registerEntry(entry)
        assertNotNull(shortcut)
        assertEquals("[EmuSync] Game Boy Advance", shortcut.appName)
        // On Windows, exe should be cmd.exe and launchOptions points to .bat
        assertTrue(shortcut.exe.lowercase().contains("cmd.exe"))
        assertTrue(shortcut.launchOptions.startsWith("/c \"\""))
        assertTrue(shortcut.launchOptions.endsWith("launch_game_boy_advance.bat\"\""))
        assertEquals("C:\\Emulators\\mGBA", shortcut.startDir)

        // Verify Windows batch launcher script
        val launcherDir = File(tempDir, "launchers")
        val batScript = File(launcherDir, "launch_game_boy_advance.bat")
        assertTrue(batScript.exists())
        val content = batScript.readText()

        assertTrue(content.startsWith("@echo off"))
        assertTrue(content.contains(":: Launcher for: Game Boy Advance"))
        assertTrue(content.contains("set \"SIGNAL_FILE="))
        assertTrue(content.contains(".current_rom_game_boy_advance\""))
        assertTrue(content.contains("set /p ROM_PATH=<\"%SIGNAL_FILE%\""))
        assertTrue(content.contains("if not exist \"%ROM_PATH%\""))
        assertTrue(content.contains("start \"\" /wait \"C:\\Emulators\\mGBA\\mgba.exe\" -f \"%ROM_PATH%\""))
        assertTrue(content.contains("exit /b %ERRORLEVEL%"))
        assertTrue(content.contains("\r\n"), "Batch file must use CRLF line endings")
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

    @Test
    fun `should parse Windows registry output accurately`() {
        val standardOutput = """
            HKEY_CURRENT_USER\Software\Valve\Steam
                SteamPath    REG_SZ    c:/program files (x86)/steam
        """.trimIndent()

        val parsedPath = SteamShortcutManager.parseRegistryOutput(standardOutput, "SteamPath")
        assertEquals("c:/program files (x86)/steam", parsedPath)

        val wow64Output = """
            HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Valve\Steam
                InstallPath    REG_SZ    C:\Program Files (x86)\Steam
        """.trimIndent()

        val parsedInstall = SteamShortcutManager.parseRegistryOutput(wow64Output, "InstallPath")
        assertEquals("C:\\Program Files (x86)\\Steam", parsedInstall)

        val expandSzOutput = """
            HKEY_CURRENT_USER\Software\Valve\Steam
                SteamPath    REG_EXPAND_SZ    %ProgramFiles%\Steam
        """.trimIndent()

        val parsedExpand = SteamShortcutManager.parseRegistryOutput(expandSzOutput, "SteamPath")
        assertEquals("%ProgramFiles%\\Steam", parsedExpand)

        val multiLineOutput = """
            HKEY_CURRENT_USER\Software\Valve\Steam
                Language     REG_SZ    english
                SteamPath    REG_SZ    D:\Games\Steam
                SteamExe     REG_SZ    D:\Games\Steam\steam.exe
        """.trimIndent()

        assertEquals("D:\\Games\\Steam", SteamShortcutManager.parseRegistryOutput(multiLineOutput, "SteamPath"))
        assertEquals("english", SteamShortcutManager.parseRegistryOutput(multiLineOutput, "Language"))
        assertEquals("D:\\Games\\Steam\\steam.exe", SteamShortcutManager.parseRegistryOutput(multiLineOutput, "SteamExe"))

        val errorOutput = "ERROR: The system was unable to find the specified registry key or value."
        assertNull(SteamShortcutManager.parseRegistryOutput(errorOutput, "SteamPath"))
        assertNull(SteamShortcutManager.parseRegistryOutput("", "SteamPath"))
    }

    @Test
    fun `should locate Windows Steam userdata directory via registry query`(@TempDir tempDir: File) {
        val mockSteamDir = File(tempDir, "Steam")
        val mockUserdata = File(mockSteamDir, "userdata")
        mockUserdata.mkdirs()

        val manager = object : SteamShortcutManager(
            registryQuery = { key, valueName ->
                if (key.contains("Valve\\Steam") && valueName == "SteamPath") {
                    mockSteamDir.absolutePath
                } else null
            }
        ) {
            override fun isWindows(): Boolean = true
        }

        val resolved = manager.getSteamUserdataDir()
        assertNotNull(resolved)
        assertEquals(mockUserdata.absolutePath, resolved.absolutePath)
    }

    @Test
    fun `should detect fullscreen flags for various emulators including RetroArch`() {
        assertEquals("-bigpicture -fullscreen", SteamShortcutManager.detectFullscreenArgs("C:\\Emulators\\duckstation-qt.exe"))
        assertEquals("-bigpicture -fullscreen", SteamShortcutManager.detectFullscreenArgs("/usr/bin/pcsx2"))
        assertEquals("-b -e", SteamShortcutManager.detectFullscreenArgs("D:\\Dolphin\\Dolphin.exe"))
        assertEquals("--fullscreen", SteamShortcutManager.detectFullscreenArgs("ppsspp.exe"))
        assertEquals("--fullscreen", SteamShortcutManager.detectFullscreenArgs("/opt/rpcs3/rpcs3"))
        assertEquals("-f", SteamShortcutManager.detectFullscreenArgs("C:\\RetroArch\\retroarch.exe"))
        assertEquals("-f", SteamShortcutManager.detectFullscreenArgs("/usr/bin/mgba-qt"))
        assertEquals("", SteamShortcutManager.detectFullscreenArgs("notepad.exe"))
    }

    @Test
    fun `prepareLaunch writes ROM path to signal file correctly`(@TempDir tempDir: File) {
        val manager = object : SteamShortcutManager(customLauncherDir = tempDir) {}
        val entry = EmulatorSystem(
            name = "Super Nintendo",
            executablePath = "/usr/bin/snes9x",
            romsDirectory = "/roms/snes",
            extensions = listOf("sfc"),
        )
        val dummyRom = File(tempDir, "Super Mario World.sfc")
        dummyRom.writeText("dummy-rom-bytes")

        manager.prepareLaunch(entry, dummyRom)

        val signalFile = manager.getSignalFile(entry)
        assertTrue(signalFile.exists())
        assertEquals(dummyRom.absolutePath, signalFile.readText())
    }

    @Test
    fun `findShortcutsFile should pick most recently active user account skipping 0`(@TempDir tempDir: File) {
        val userdataDir = File(tempDir, "userdata")
        val offlineDir = File(userdataDir, "0/config").apply { mkdirs() }
        val user1Dir = File(userdataDir, "111111/config").apply { mkdirs() }
        val user2Dir = File(userdataDir, "222222/config").apply { mkdirs() }

        File(offlineDir, "shortcuts.vdf").writeText("offline")
        val olderVdf = File(user1Dir, "shortcuts.vdf").apply { writeText("older") }
        val newerVdf = File(user2Dir, "shortcuts.vdf").apply { writeText("newer") }

        // Set timestamps
        olderVdf.parentFile.parentFile.setLastModified(1000L)
        newerVdf.parentFile.parentFile.setLastModified(2000L)

        val manager = object : SteamShortcutManager(customSteamUserdataDir = userdataDir) {}
        val found = manager.findShortcutsFile()

        assertNotNull(found)
        assertEquals(newerVdf.absolutePath, found.absolutePath)
        assertFalse(found.absolutePath.contains("/0/"))
    }
}
