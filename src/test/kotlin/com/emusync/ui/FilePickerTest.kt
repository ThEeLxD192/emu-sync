package com.emusync.ui

import com.emusync.ui.components.ExternalChooserResult
import com.emusync.ui.components.FilePicker
import com.emusync.ui.components.PickerType
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File

class FilePickerTest {

    private val originalIsWindows = FilePicker.isWindowsProvider
    private val originalFileChooser = FilePicker.fileChooser
    private val originalSwingDirectoryChooser = FilePicker.swingDirectoryChooser
    private val originalLinuxExternalDirectoryChooser = FilePicker.linuxExternalDirectoryChooser
    private val originalLinuxExternalFileChooser = FilePicker.linuxExternalFileChooser

    @BeforeEach
    fun setUp() {
        FilePicker.isWindowsProvider = originalIsWindows
        FilePicker.fileChooser = originalFileChooser
        FilePicker.swingDirectoryChooser = originalSwingDirectoryChooser
        FilePicker.linuxExternalDirectoryChooser = originalLinuxExternalDirectoryChooser
        FilePicker.linuxExternalFileChooser = originalLinuxExternalFileChooser
    }

    @AfterEach
    fun tearDown() {
        FilePicker.isWindowsProvider = originalIsWindows
        FilePicker.fileChooser = originalFileChooser
        FilePicker.swingDirectoryChooser = originalSwingDirectoryChooser
        FilePicker.linuxExternalDirectoryChooser = originalLinuxExternalDirectoryChooser
        FilePicker.linuxExternalFileChooser = originalLinuxExternalFileChooser
    }

    @Test
    fun `pickPath returns null for PickerType ANY`() {
        val result = FilePicker.pickPath("Test", "/path", PickerType.ANY)
        assertNull(result)
    }

    @Test
    fun `pickPath delegates to fileChooser for PickerType FILE on Windows`(@TempDir tempDir: File) {
        FilePicker.isWindowsProvider = { true }
        val testFile = File(tempDir, "game.rom").apply { writeText("data") }
        var invoked = false

        FilePicker.fileChooser = { label, parentDir, startFile ->
            invoked = true
            assertEquals("ROM File", label)
            assertEquals(tempDir.absolutePath, parentDir.absolutePath)
            assertEquals(testFile.name, startFile.name)
            "/chosen/game.rom"
        }

        val result = FilePicker.pickPath("ROM File", testFile.absolutePath, PickerType.FILE)
        assertTrue(invoked)
        assertEquals("/chosen/game.rom", result)
    }

    @Test
    fun `pickFile uses Linux external tool when available`(@TempDir tempDir: File) {
        FilePicker.isWindowsProvider = { false }
        val testFile = File(tempDir, "cover.png").apply { writeText("img") }
        var externalInvoked = false
        FilePicker.linuxExternalFileChooser = { label, parentDir, startFile ->
            externalInvoked = true
            assertEquals("Cover Image", label)
            assertEquals(tempDir.absolutePath, parentDir.absolutePath)
            assertEquals(testFile.name, startFile.name)
            ExternalChooserResult.Selected("/chosen/cover.png")
        }

        var awtInvoked = false
        FilePicker.fileChooser = { _, _, _ ->
            awtInvoked = true
            null
        }

        val result = FilePicker.pickPath("Cover Image", testFile.absolutePath, PickerType.FILE)
        assertTrue(externalInvoked)
        assertFalse(awtInvoked)
        assertEquals("/chosen/cover.png", result)
    }

    @Test
    fun `pickFile returns null and does not fall back to AWT when Linux external tool is cancelled`(@TempDir tempDir: File) {
        FilePicker.isWindowsProvider = { false }
        val testFile = File(tempDir, "cover.png").apply { writeText("img") }
        FilePicker.linuxExternalFileChooser = { _, _, _ -> ExternalChooserResult.Cancelled }

        var awtInvoked = false
        FilePicker.fileChooser = { _, _, _ ->
            awtInvoked = true
            "/fallback/cover.png"
        }

        val result = FilePicker.pickPath("Cover Image", testFile.absolutePath, PickerType.FILE)
        assertNull(result)
        assertFalse(awtInvoked)
    }

    @Test
    fun `pickFile falls back to fileChooser on Linux when external tools are not available`(@TempDir tempDir: File) {
        FilePicker.isWindowsProvider = { false }
        val testFile = File(tempDir, "cover.png").apply { writeText("img") }
        FilePicker.linuxExternalFileChooser = { _, _, _ -> ExternalChooserResult.NotAvailable }

        var awtInvoked = false
        FilePicker.fileChooser = { label, parentDir, _ ->
            awtInvoked = true
            assertEquals("Cover Image", label)
            assertEquals(tempDir.absolutePath, parentDir.absolutePath)
            "/fallback/cover.png"
        }

        val result = FilePicker.pickPath("Cover Image", testFile.absolutePath, PickerType.FILE)
        assertTrue(awtInvoked)
        assertEquals("/fallback/cover.png", result)
    }

    @Test
    fun `pickDirectory delegates directly to Swing chooser on Windows`(@TempDir tempDir: File) {
        FilePicker.isWindowsProvider = { true }

        var swingInvoked = false
        FilePicker.swingDirectoryChooser = { label, parentDir ->
            swingInvoked = true
            assertEquals("ROM Directory", label)
            assertEquals(tempDir.absolutePath, parentDir.absolutePath)
            "C:\\Games\\ROMs"
        }

        val result = FilePicker.pickPath("ROM Directory", tempDir.absolutePath, PickerType.DIRECTORY)
        assertTrue(swingInvoked)
        assertEquals("C:\\Games\\ROMs", result)
    }

    @Test
    fun `pickDirectory uses Linux external tool when available`(@TempDir tempDir: File) {
        FilePicker.isWindowsProvider = { false }
        FilePicker.linuxExternalDirectoryChooser = { label, _ ->
            assertEquals("Linux Folder", label)
            ExternalChooserResult.Selected("/custom/linux/path")
        }

        var swingInvoked = false
        FilePicker.swingDirectoryChooser = { _, _ ->
            swingInvoked = true
            null
        }

        val result = FilePicker.pickPath("Linux Folder", tempDir.absolutePath, PickerType.DIRECTORY)
        assertEquals("/custom/linux/path", result)
        assertFalse(swingInvoked)
    }

    @Test
    fun `pickDirectory returns null and does not fall back to Swing when Linux external tool is cancelled`(@TempDir tempDir: File) {
        FilePicker.isWindowsProvider = { false }
        FilePicker.linuxExternalDirectoryChooser = { _, _ -> ExternalChooserResult.Cancelled }

        var swingInvoked = false
        FilePicker.swingDirectoryChooser = { _, _ ->
            swingInvoked = true
            "/fallback/path"
        }

        val result = FilePicker.pickPath("Linux Folder", tempDir.absolutePath, PickerType.DIRECTORY)
        assertNull(result)
        assertFalse(swingInvoked)
    }

    @Test
    fun `pickDirectory falls back to Swing chooser on Linux when external tools are not available`(@TempDir tempDir: File) {
        FilePicker.isWindowsProvider = { false }
        FilePicker.linuxExternalDirectoryChooser = { _, _ -> ExternalChooserResult.NotAvailable }

        var swingInvoked = false
        FilePicker.swingDirectoryChooser = { label, parentDir ->
            swingInvoked = true
            assertEquals("Saves Directory", label)
            assertEquals(tempDir.absolutePath, parentDir.absolutePath)
            "/home/user/saves"
        }

        val result = FilePicker.pickPath("Saves Directory", tempDir.absolutePath, PickerType.DIRECTORY)
        assertTrue(swingInvoked)
        assertEquals("/home/user/saves", result)
    }

    @Test
    fun `parentDir defaults to user home when initialPath is blank`() {
        FilePicker.isWindowsProvider = { true }
        var resolvedParent: File? = null
        FilePicker.fileChooser = { _, parentDir, _ ->
            resolvedParent = parentDir
            null
        }

        FilePicker.pickPath("Select", "", PickerType.FILE)
        assertEquals(File(System.getProperty("user.home")).absolutePath, resolvedParent?.absolutePath)
    }
}
