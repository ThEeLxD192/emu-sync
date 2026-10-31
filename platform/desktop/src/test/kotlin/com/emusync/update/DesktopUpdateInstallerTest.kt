package com.emusync.update

import java.io.File
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Assumptions.assumeFalse
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class DesktopUpdateInstallerTest {
    @Test
    fun `applyWindowsUpdateAndRestart launches msiexec for MSI files`(@TempDir tempDir: File) {
        val manager = DesktopUpdateInstaller()
        val msiFile = File(tempDir, "EmuSync-Setup.msi").apply { createNewFile() }

        var executedCommand: List<String>? = null
        val success =
            manager.applyWindowsUpdateAndRestart(msiFile) { cmd ->
                executedCommand = cmd
            }

        assertTrue(success)
        assertNotNull(executedCommand)
        assertEquals(listOf("msiexec", "/i", msiFile.absolutePath), executedCommand)
    }

    @Test
    fun `applyWindowsUpdateAndRestart launches EXE installer directly`(@TempDir tempDir: File) {
        val manager = DesktopUpdateInstaller()
        val exeFile = File(tempDir, "EmuSync-Setup.exe").apply { createNewFile() }

        var executedCommand: List<String>? = null
        val success =
            manager.applyWindowsUpdateAndRestart(exeFile) { cmd ->
                executedCommand = cmd
            }

        assertTrue(success)
        assertNotNull(executedCommand)
        assertEquals(listOf(exeFile.absolutePath), executedCommand)
    }

    @Test
    fun `applyWindowsUpdateAndRestart returns false for non-installer files`(
        @TempDir tempDir: File
    ) {
        val manager = DesktopUpdateInstaller()
        val txtFile = File(tempDir, "readme.txt").apply { createNewFile() }

        var launched = false
        val success =
            manager.applyWindowsUpdateAndRestart(txtFile) { _ ->
                launched = true
            }

        assertFalse(success)
        assertFalse(launched)
    }

    @Test
    fun `applyLinuxUpdateAndRestart replaces AppImage file and calls launcher`(
        @TempDir tempDir: File
    ) {
        val currentAppImage = File(tempDir, "EmuSync.AppImage").apply { writeText("OLD_VERSION") }
        val downloadedUpdate = File(tempDir, ".update.AppImage").apply { writeText("NEW_VERSION") }

        var launchedPath: String? = null
        val manager =
            object : DesktopUpdateInstaller() {
                override fun isWindows() = false

                override fun isSteamGameMode() = false

                override fun getCurrentAppImagePath() = currentAppImage.absolutePath
            }

        val success =
            manager.applyLinuxUpdateAndRestart(downloadedUpdate) { path ->
                launchedPath = path
            }

        assertTrue(success)
        assertEquals(currentAppImage.absolutePath, launchedPath)
        assertEquals("NEW_VERSION", currentAppImage.readText())
        assertFalse(downloadedUpdate.exists())
    }

    @Test
    fun `applyLinuxUpdateAndRestart returns false when APPIMAGE path is missing`(
        @TempDir tempDir: File
    ) {
        val downloadedUpdate = File(tempDir, ".update.AppImage").apply { writeText("NEW_VERSION") }

        var launched = false
        val manager =
            object : DesktopUpdateInstaller() {
                override fun isWindows() = false

                override fun getCurrentAppImagePath(): String? = null
            }

        val success =
            manager.applyLinuxUpdateAndRestart(downloadedUpdate) { _ ->
                launched = true
            }

        assertFalse(success)
        assertFalse(launched)
    }

    @Test
    fun `applyLinuxUpdateAndRestart returns false when downloaded file does not exist`(
        @TempDir tempDir: File
    ) {
        val currentAppImage = File(tempDir, "EmuSync.AppImage").apply { writeText("OLD_VERSION") }
        val nonExistentUpdate = File(tempDir, "does-not-exist.AppImage")

        var launched = false
        val manager =
            object : DesktopUpdateInstaller() {
                override fun isWindows() = false

                override fun getCurrentAppImagePath() = currentAppImage.absolutePath
            }

        val success =
            manager.applyLinuxUpdateAndRestart(nonExistentUpdate) { _ ->
                launched = true
            }

        assertFalse(success)
        assertFalse(launched)
    }

    @Test
    fun `applyUpdateAndRestart delegates to Linux update on Linux platform`(
        @TempDir tempDir: File
    ) {
        File(tempDir, "EmuSync.AppImage").writeText("V1")
        val downloadedUpdate = File(tempDir, "update.AppImage").apply { writeText("V2") }

        var linuxUpdateCalled = false
        val manager =
            object : DesktopUpdateInstaller(exitApplication = {}) {
                override fun isWindows() = false

                override fun applyLinuxUpdateAndRestart(
                    downloadedFile: File,
                    launcher: (String) -> Unit,
                ): Boolean {
                    linuxUpdateCalled = true
                    return true
                }
            }

        val success = manager.applyUpdateAndRestart(downloadedUpdate)
        assertTrue(success)
        assertTrue(linuxUpdateCalled)
    }

    @Test
    fun `applyLinuxUpdateAndRestart in Steam Game Mode calls launcher with updated path`(
        @TempDir tempDir: File
    ) {
        val currentAppImage = File(tempDir, "EmuSync.AppImage").apply { writeText("V1") }
        val downloadedUpdate = File(tempDir, "update.AppImage").apply { writeText("V2") }

        var launchedPath: String? = null
        val manager =
            object : DesktopUpdateInstaller() {
                override fun isWindows() = false

                override fun isSteamGameMode() = true

                override fun getCurrentAppImagePath() = currentAppImage.absolutePath
            }

        val success =
            manager.applyLinuxUpdateAndRestart(downloadedUpdate) { path ->
                launchedPath = path
            }

        assertTrue(success)
        assertEquals(currentAppImage.absolutePath, launchedPath)
        assertEquals("V2", currentAppImage.readText())
    }

    @Test
    fun `isSteamGameMode returns false by default in non-gaming test environment`() {
        assumeFalse(
            System.getenv("SteamGamepadUI") == "1" ||
                System.getenv("XDG_CURRENT_DESKTOP")?.equals("gamescope", ignoreCase = true) ==
                    true,
            "The default desktop assertion requires a session outside Steam Game Mode",
        )
        assertFalse(DesktopUpdateInstaller().isSteamGameMode())
    }
}
