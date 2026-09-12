package com.emusync.runner

import com.emusync.model.EmulatorSystem
import com.emusync.model.NativePCGame
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.assertThrows
import org.junit.jupiter.api.io.TempDir
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class GameRunnerTest {

    private val runner = GameRunner()

    @Test
    fun `should fail when EmulatorSystem is launched without romFile`() = runTest {
        val emu = EmulatorSystem(
            name = "Test Emu",
            executablePath = "echo",
            romsDirectory = "/tmp",
            extensions = listOf("bin"),
        )

        val exception = assertThrows<IllegalArgumentException> {
            runner.launch(emu, romFile = null)
        }
        assertTrue(exception.message!!.contains("A ROM file must be provided"))
    }

    @Test
    fun `should successfully launch and wait for a direct command`(@TempDir tempDir: File) = runTest {
        val testFile = File(tempDir, "test_rom.bin").also { it.createNewFile() }

        val emu = EmulatorSystem(
            name = "Echo Emu",
            executablePath = "echo",
            arguments = listOf("Running", "{ROM}"),
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("bin"),
        )

        val result = runner.launch(emu, romFile = testFile)
        assertEquals(0, result.exitCode)
        assertTrue(result.durationMs >= 0)
    }

    @Test
    fun `should successfully launch native game direct command`() = runTest {
        val nativeGame = NativePCGame(
            name = "Echo Game",
            executablePath = "echo",
            arguments = listOf("Hello", "World"),
            savePaths = emptyList(),
        )

        val result = runner.launch(nativeGame)
        assertEquals(0, result.exitCode)
        assertTrue(result.durationMs >= 0)
    }

    @Test
    fun `isProcessRunning should return false for blank or non-existent process`() {
        assertEquals(false, runner.isProcessRunning(""))
        assertEquals(false, runner.isProcessRunning("   "))
        assertEquals(false, runner.isProcessRunning("definitely_non_existent_process_987654"))
    }

    @Test
    fun `isProcessRunning should detect active process and handle lifecycle`() {
        val process = ProcessBuilder("sleep", "5").start()
        try {
            assertTrue(runner.isProcessRunning("sleep"), "Should detect running sleep process")
            assertTrue(runner.isProcessRunning("\"sleep\""), "Should handle quoted process name")
        } finally {
            process.destroyForcibly()
            process.waitFor()
        }
    }

    @Test
    fun `should wait for external process when launcher exits early`(@TempDir tempDir: File) = runTest {
        val gameScript = File(tempDir, "real_game.sh").apply {
            writeText("#!/bin/sh\nsleep 2\n")
            setExecutable(true)
        }
        val launcherScript = File(tempDir, "launcher.sh").apply {
            writeText("#!/bin/sh\n\"${gameScript.absolutePath}\" &\nexit 0\n")
            setExecutable(true)
        }

        val nativeGame = NativePCGame(
            name = "Async Game",
            executablePath = launcherScript.absolutePath,
            waitForProcess = "real_game.sh"
        )

        val start = System.currentTimeMillis()
        val result = runner.launch(nativeGame)
        val elapsed = System.currentTimeMillis() - start

        assertEquals(0, result.exitCode)
        assertTrue(elapsed >= 1800, "Runner should have waited for real_game.sh to complete (elapsed: ${elapsed}ms)")
    }
}
