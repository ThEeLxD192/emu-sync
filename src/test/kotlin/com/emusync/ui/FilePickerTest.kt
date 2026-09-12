package com.emusync.ui

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
    private val originalLinuxExternalChooser = FilePicker.linuxExternalDirectoryChooser

    @BeforeEach
    fun setUp() {
        FilePicker.isWindowsProvider = originalIsWindows
        FilePicker.fileChooser = originalFileChooser
        FilePicker.swingDirectoryChooser = originalSwingDirectoryChooser
        FilePicker.linuxExternalDirectoryChooser = originalLinuxExternalChooser
    }

    @AfterEach
    fun tearDown() {
        FilePicker.isWindowsProvider = originalIsWindows
        FilePicker.fileChooser = originalFileChooser
        FilePicker.swingDirectoryChooser = originalSwingDirectoryChooser
        FilePicker.linuxExternalDirectoryChooser = originalLinuxExternalChooser
    }

    @Test
    fun `pickPath returns null for PickerType ANY`() {
        val result = FilePicker.pickPath("Test", "/path", PickerType.ANY)
        assertNull(result)
    }

    @Test
    fun `pickPath delegates to fileChooser for PickerType FILE`(@TempDir tempDir: File) {
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
            "/custom/linux/path"
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
    fun `pickDirectory falls back to Swing chooser on Linux when external tools fail`(@TempDir tempDir: File) {
        FilePicker.isWindowsProvider = { false }
        FilePicker.linuxExternalDirectoryChooser = { _, _ -> null }

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
        var resolvedParent: File? = null
        FilePicker.fileChooser = { _, parentDir, _ ->
            resolvedParent = parentDir
            null
        }

        FilePicker.pickPath("Select", "", PickerType.FILE)
        assertEquals(File(System.getProperty("user.home")).absolutePath, resolvedParent?.absolutePath)
    }
}
