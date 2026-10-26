package com.emusync.update

import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import java.io.IOException
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue

class AppImageInstallerTest {
    @Test
    fun `keeps original and download until launch succeeds`(@TempDir directory: File) {
        val target = File(directory, "EmuSync.AppImage").apply { writeText("ORIGINAL") }
        val source = File(directory, "download.AppImage").apply { writeText("UPDATE") }

        AppImageInstaller().replace(source, target) {
            assertEquals("UPDATE", target.readText())
            assertTrue(target.canExecute())
            assertEquals("UPDATE", source.readText())
            val backup = directory.listFiles()!!.single { it.extension == "bak" }
            assertEquals("ORIGINAL", backup.readText())
        }

        assertEquals("UPDATE", target.readText())
        assertEquals(listOf(target.name), directory.list()!!.toList())
    }

    @Test
    fun `partial staging copy failure preserves original and download`(@TempDir directory: File) {
        val target = File(directory, "EmuSync.AppImage").apply { writeText("ORIGINAL") }
        val source = File(directory, "download.AppImage").apply { writeText("UPDATE") }
        val installer = AppImageInstaller(copyFile = { _, staged ->
            Files.writeString(staged, "PARTIAL")
            throw IOException("Disk full")
        })
        var launched = false

        assertFailsWith<IOException> {
            installer.replace(source, target) { launched = true }
        }

        assertFalse(launched)
        assertEquals("ORIGINAL", target.readText())
        assertEquals("UPDATE", source.readText())
        assertEquals(setOf(target.name, source.name), directory.list()!!.toSet())
    }

    @Test
    fun `failed promotion restores original and retains download`(@TempDir directory: File) {
        val target = File(directory, "EmuSync.AppImage").apply { writeText("ORIGINAL") }
        val source = File(directory, "download.AppImage").apply { writeText("UPDATE") }
        val installer = AppImageInstaller(moveFile = { from, to ->
            if (from.fileName.toString().contains(".update.")) {
                throw IOException("Could not promote staged update")
            }
            Files.move(from, to, StandardCopyOption.REPLACE_EXISTING)
        })

        assertFailsWith<IOException> { installer.replace(source, target) }

        assertEquals("ORIGINAL", target.readText())
        assertEquals("UPDATE", source.readText())
        assertEquals(setOf(target.name, source.name), directory.list()!!.toSet())
    }

    @Test
    fun `failed launcher restores original and retains download`(@TempDir directory: File) {
        val target = File(directory, "EmuSync.AppImage").apply { writeText("ORIGINAL") }
        val source = File(directory, "download.AppImage").apply { writeText("UPDATE") }

        assertFailsWith<IOException> {
            AppImageInstaller().replace(source, target) { throw IOException("Could not start process") }
        }

        assertEquals("ORIGINAL", target.readText())
        assertEquals("UPDATE", source.readText())
        assertEquals(setOf(target.name, source.name), directory.list()!!.toSet())
    }

    @Test
    fun `failed rollback keeps backup available for recovery`(@TempDir directory: File) {
        val target = File(directory, "EmuSync.AppImage").apply { writeText("ORIGINAL") }
        val source = File(directory, "download.AppImage").apply { writeText("UPDATE") }
        val installer = AppImageInstaller(moveFile = { from, to ->
            if (from != target.toPath()) throw IOException("Filesystem unavailable")
            Files.move(from, to, StandardCopyOption.REPLACE_EXISTING)
        })

        val failure = assertFailsWith<IOException> { installer.replace(source, target) }

        val backup = directory.listFiles()!!.single { it.extension == "bak" }
        assertEquals("ORIGINAL", backup.readText())
        assertEquals("UPDATE", source.readText())
        assertTrue(failure.suppressed.single().message!!.contains(backup.absolutePath))
        assertFalse(directory.list()!!.any { it.endsWith(".tmp") })
    }

    @Test
    fun `rejects replacing a file with itself`(@TempDir directory: File) {
        val target = File(directory, "EmuSync.AppImage").apply { writeText("ORIGINAL") }

        assertFailsWith<IllegalArgumentException> { AppImageInstaller().replace(target, target) }

        assertEquals("ORIGINAL", target.readText())
        assertEquals(listOf(target.name), directory.list()!!.toList())
    }
}
