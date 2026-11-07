package com.emusync.update

import java.io.File
import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.condition.EnabledIfEnvironmentVariable
import org.junit.jupiter.api.condition.EnabledOnOs
import org.junit.jupiter.api.condition.OS
import org.junit.jupiter.api.io.TempDir

class UpdateRestartTest {
    @Test
    @EnabledOnOs(OS.LINUX)
    @EnabledIfEnvironmentVariable(named = "EMUSYNC_R8_TRANSITION", matches = "1")
    fun `Kotlin installer replaces isolated Kotlin AppImage with Rust and launches it`(
        @TempDir directory: File
    ) {
        val kotlinArtifact = File(requireNotNull(System.getenv("EMUSYNC_R8_KOTLIN_IMAGE")))
        val rustArtifact = File(requireNotNull(System.getenv("EMUSYNC_R8_RUST_IMAGE")))
        val ready = File(requireNotNull(System.getenv("EMUSYNC_R8_LAUNCH_READY")))
        val finished = File(requireNotNull(System.getenv("EMUSYNC_R8_LAUNCH_FINISHED")))
        val config = File(requireNotNull(System.getenv("EMUSYNC_CONFIG")))
        assertEquals(ready.canonicalFile.parentFile, finished.canonicalFile.parentFile)
        assertEquals(ready.canonicalFile.parentFile, config.canonicalFile.parentFile)
        assertEquals("{}", config.readText())
        assertFalse(ready.exists())
        assertFalse(finished.exists())
        val target = File(directory, "EmuSync-x86_64.AppImage")
        val download = File(directory, "download.AppImage")
        kotlinArtifact.copyTo(target)
        rustArtifact.copyTo(download)
        val expected = rustArtifact.readBytes()
        val exits = mutableListOf<Int>()
        val manager =
            object : DesktopUpdateInstaller(exitApplication = { exits += it }) {
                override fun isWindows() = false

                override fun isSteamGameMode() = false

                override fun getCurrentAppImagePath() = target.absolutePath

                override fun defaultLinuxLauncher(path: String) {
                    super.defaultLinuxLauncher(path)
                    ready.writeText("${ProcessHandle.current().pid()}\n$path\n")
                    val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(30)
                    while (!finished.exists() && System.nanoTime() < deadline) Thread.sleep(10)
                    if (!finished.exists())
                        throw IOException("Timed out waiting for the isolated launch")
                }
            }

        assertTrue(manager.applyUpdateAndRestart(download))
        assertEquals(listOf(0), exits)
        assertTrue(expected.contentEquals(target.readBytes()))
        assertTrue(target.canExecute())
        assertFalse(download.exists())
        assertEquals(listOf(target.name), directory.list()!!.toList())
        assertEquals("{}", config.readText())
    }

    @Test
    @EnabledOnOs(OS.LINUX)
    fun `real Linux launcher starts the updated file before exiting`(@TempDir directory: File) {
        val target = File(directory, "Emu Sync \$literal.AppImage").apply { writeText("ORIGINAL") }
        val marker = File(directory, "started.txt")
        val script = "#!/bin/sh\nprintf 'started' > '${marker.absolutePath}'\n"
        val source = File(directory, "download.AppImage").apply { writeText(script) }
        val exits = mutableListOf<Int>()
        val manager =
            object :
                DesktopUpdateInstaller(
                    exitApplication = { code ->
                        assertEquals(script, target.readText())
                        assertFalse(source.exists())
                        assertFalse(
                            directory.list()!!.any { it.endsWith(".bak") || it.endsWith(".tmp") }
                        )
                        exits += code
                    }
                ) {
                override fun isWindows() = false

                override fun isSteamGameMode() = false

                override fun getCurrentAppImagePath() = target.absolutePath
            }

        assertTrue(manager.applyUpdateAndRestart(source))
        assertEquals(listOf(0), exits)

        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(5)
        while ((!marker.exists() || marker.length() == 0L) && System.nanoTime() < deadline) {
            Thread.sleep(10)
        }
        assertTrue(marker.exists(), "The real launcher must execute the updated file")
        assertEquals("started", marker.readText())
    }

    @Test
    fun `launch failure restores current AppImage and keeps application open`(
        @TempDir directory: File
    ) {
        val target = File(directory, "EmuSync.AppImage").apply { writeText("ORIGINAL") }
        val source = File(directory, "download.AppImage").apply { writeText("UPDATE") }
        val exits = mutableListOf<Int>()
        val manager =
            object : DesktopUpdateInstaller(exitApplication = { exits += it }) {
                override fun isWindows() = false

                override fun getCurrentAppImagePath() = target.absolutePath

                override fun defaultLinuxLauncher(path: String) {
                    assertEquals("UPDATE", File(path).readText())
                    throw IOException("Process creation failed")
                }
            }

        assertFalse(manager.applyUpdateAndRestart(source))

        assertTrue(exits.isEmpty())
        assertEquals("ORIGINAL", target.readText())
        assertEquals("UPDATE", source.readText())
        assertEquals(setOf(target.name, source.name), directory.list()!!.toSet())
    }

    @Test
    fun `Steam Game Mode finishes installation before exit without starting a process`(
        @TempDir directory: File
    ) {
        val target = File(directory, "EmuSync.AppImage").apply { writeText("ORIGINAL") }
        val source = File(directory, "download.AppImage").apply { writeText("UPDATE") }
        val exits = mutableListOf<Int>()
        val manager =
            object :
                DesktopUpdateInstaller(
                    exitApplication = { code ->
                        assertEquals("UPDATE", target.readText())
                        assertEquals(listOf(target.name), directory.list()!!.toList())
                        exits += code
                    }
                ) {
                override fun isWindows() = false

                override fun isSteamGameMode() = true

                override fun getCurrentAppImagePath() = target.absolutePath
            }

        assertTrue(manager.applyUpdateAndRestart(source))
        assertEquals(listOf(0), exits)
    }

    @Test
    fun `failed Windows installer launch keeps application open`(@TempDir directory: File) {
        val source = File(directory, "setup.msi").apply { writeText("INSTALLER") }
        val exits = mutableListOf<Int>()
        val manager =
            object : DesktopUpdateInstaller(exitApplication = { exits += it }) {
                override fun isWindows() = true

                override fun applyWindowsUpdateAndRestart(
                    downloadedFile: File,
                    launcher: (List<String>) -> Unit,
                ): Boolean =
                    super.applyWindowsUpdateAndRestart(downloadedFile) {
                        throw IOException("Process creation failed")
                    }
            }

        assertFalse(manager.applyUpdateAndRestart(source))
        assertTrue(exits.isEmpty())
        assertTrue(source.exists())
    }

    @Test
    fun `successful Windows installer launch exits afterwards`(@TempDir directory: File) {
        val source = File(directory, "setup.msi").apply { writeText("INSTALLER") }
        val events = mutableListOf<String>()
        val manager =
            object : DesktopUpdateInstaller(exitApplication = { events += "exit:$it" }) {
                override fun isWindows() = true

                override fun applyWindowsUpdateAndRestart(
                    downloadedFile: File,
                    launcher: (List<String>) -> Unit,
                ): Boolean =
                    super.applyWindowsUpdateAndRestart(downloadedFile) {
                        events += "launch:${it.first()}"
                    }
            }

        assertTrue(manager.applyUpdateAndRestart(source))
        assertEquals(listOf("launch:msiexec", "exit:0"), events)
    }
}
