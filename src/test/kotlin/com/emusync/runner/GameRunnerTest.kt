package com.emusync.runner

import com.emusync.model.EmulatorSystem
import com.emusync.model.NativePCGame
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.assertThrows
import org.junit.jupiter.api.io.TempDir
import org.junit.jupiter.api.Assumptions.assumeFalse
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class GameRunnerTest {

    private val runner = GameRunner()

    @Test
    fun `openEmulator starts without rom arguments`(@TempDir tempDir: File) = runTest {
        assumeFalse(System.getProperty("os.name", "").lowercase().contains("windows"))
        val argsFile = File(tempDir, "args.txt")
        val emulatorScript = File(tempDir, "emulator.sh").apply {
            writeText("#!/bin/sh\nprintf '%s\\n' \"\$@\" > '${argsFile.absolutePath}'\n")
            setExecutable(true)
        }
        val emulator = EmulatorSystem(
            name = "Test Emulator",
            executablePath = emulatorScript.absolutePath,
            arguments = listOf("--gui", "--load={ROM}"),
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("bin"),
        )

        runner.openEmulator(emulator)
        repeat(50) {
            if (argsFile.exists()) return@repeat
            kotlinx.coroutines.delay(20)
        }

        assertEquals("--gui", argsFile.readText().trim())
    }

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
        val isWindows = System.getProperty("os.name", "").lowercase().contains("windows")
        val (process, procName) = if (isWindows) {
            ProcessBuilder("ping", "127.0.0.1", "-n", "6").start() to "ping"
        } else {
            ProcessBuilder("sleep", "5").start() to "sleep"
        }
        try {
            assertTrue(runner.isProcessRunning(procName), "Should detect running $procName process")
            assertTrue(runner.isProcessRunning("\"$procName\""), "Should handle quoted process name")
        } finally {
            process.destroyForcibly()
            process.waitFor()
        }
    }

    @Test
    fun `should wait for external process when launcher exits early`(@TempDir tempDir: File) = runTest {
        val isWindows = System.getProperty("os.name", "").lowercase().contains("windows")
        val nativeGame = if (isWindows) {
            val pingExe = File(System.getenv("SystemRoot") ?: "C:\\Windows", "System32\\ping.exe")
            val gameExe = File(tempDir, "real_game.exe")
            pingExe.copyTo(gameExe, overwrite = true)

            val launcherBat = File(tempDir, "launcher.bat").apply {
                writeText("@echo off\r\nstart \"\" \"${gameExe.absolutePath}\" 127.0.0.1 -n 3\r\nexit 0\r\n")
            }

            NativePCGame(
                name = "Async Game",
                executablePath = "cmd.exe",
                arguments = listOf("/c", launcherBat.absolutePath),
                waitForProcess = "real_game"
            )
        } else {
            val gameScript = File(tempDir, "real_game.sh").apply {
                writeText("#!/bin/sh\nsleep 2\n")
                setExecutable(true)
            }
            val launcherScript = File(tempDir, "launcher.sh").apply {
                writeText("#!/bin/sh\n\"${gameScript.absolutePath}\" &\nexit 0\n")
                setExecutable(true)
            }

            NativePCGame(
                name = "Async Game",
                executablePath = launcherScript.absolutePath,
                waitForProcess = "real_game.sh"
            )
        }

        val start = System.currentTimeMillis()
        val result = runner.launch(nativeGame)
        val elapsed = System.currentTimeMillis() - start

        assertEquals(0, result.exitCode)
        assertTrue(elapsed >= 1800, "Runner should have waited for external process to complete (elapsed: ${elapsed}ms)")
    }

    @Test
    fun `existing process with the same name does not delay newly launched game`(@TempDir tempDir: File) = runTest {
        assumeFalse(System.getProperty("os.name", "").lowercase().contains("windows"))
        val gameScript = File(tempDir, "tracked_game.sh").apply {
            writeText("#!/bin/sh\nsleep \"\$1\"\n")
            setExecutable(true)
        }
        val decoy = ProcessBuilder(gameScript.absolutePath, "6").start()
        val launcher = File(tempDir, "launcher.sh").apply {
            writeText("#!/bin/sh\n\"${gameScript.absolutePath}\" 1 &\nexit 0\n")
            setExecutable(true)
        }
        val game = NativePCGame(
            name = "Tracked Game",
            executablePath = launcher.absolutePath,
            waitForProcess = gameScript.name,
        )

        try {
            val started = System.currentTimeMillis()
            val result = runner.launch(game)
            val elapsed = System.currentTimeMillis() - started

            assertEquals(0, result.exitCode)
            assertTrue(elapsed < 5_000, "Runner waited for the pre-existing decoy process ($elapsed ms)")
            assertTrue(decoy.isAlive, "The decoy should still be alive when tracking completes")
        } finally {
            decoy.destroyForcibly()
            decoy.waitFor()
        }
    }

    @Test
    fun `existing single instance process is tracked when launcher reuses it`(@TempDir tempDir: File) = runTest {
        assumeFalse(System.getProperty("os.name", "").lowercase().contains("windows"))
        val gameScript = File(tempDir, "single_instance_game.sh").apply {
            writeText("#!/bin/sh\nsleep \"\$1\"\n")
            setExecutable(true)
        }
        val existingGame = ProcessBuilder(gameScript.absolutePath, "1.2").start()

        try {
            var existingProcessIds = emptySet<Long>()
            for (attempt in 0 until 50) {
                existingProcessIds = runner.matchingProcessIds(gameScript.name)
                if (existingProcessIds.isNotEmpty()) break
                kotlinx.coroutines.delay(10)
            }
            assertTrue(existingProcessIds.isNotEmpty(), "The existing game process should be detectable")

            val launchRequest = ProcessBuilder("sh", "-c", "exit 0").start()
            val started = System.currentTimeMillis()
            val launched = runner.waitForExternalProcess(
                processName = gameScript.name,
                existingProcessIds = existingProcessIds,
                launchRequest = launchRequest,
                appearanceTimeoutMs = 3_000L,
                pollIntervalMs = 10L,
                exitPollIntervalMs = 10L,
                reuseConfirmationDelayMs = 100L,
            )
            val elapsed = System.currentTimeMillis() - started

            assertTrue(launched, "The existing single-instance game should count as a successful launch")
            assertTrue(elapsed >= 800, "Runner should wait for the reused game to exit (elapsed: ${elapsed}ms)")
            assertTrue(!existingGame.isAlive)
        } finally {
            existingGame.destroyForcibly()
            existingGame.waitFor()
        }
    }

    @Test
    fun `successful desktop handoff can confirm a reused process`(@TempDir tempDir: File) = runTest {
        assumeFalse(System.getProperty("os.name", "").lowercase().contains("windows"))
        val gameScript = File(tempDir, "desktop_handoff_game.sh").apply {
            writeText("#!/bin/sh\nsleep \"\$1\"\n")
            setExecutable(true)
        }
        val existingGame = ProcessBuilder(gameScript.absolutePath, "1.2").start()

        try {
            var existingProcessIds = emptySet<Long>()
            for (attempt in 0 until 50) {
                existingProcessIds = runner.matchingProcessIds(gameScript.name)
                if (existingProcessIds.isNotEmpty()) break
                kotlinx.coroutines.delay(10)
            }
            assertTrue(existingProcessIds.isNotEmpty(), "The existing game process should be detectable")

            val started = System.currentTimeMillis()
            val launched = runner.waitForExternalProcess(
                processName = gameScript.name,
                existingProcessIds = existingProcessIds,
                launchHandoffSucceeded = true,
                appearanceTimeoutMs = 3_000L,
                pollIntervalMs = 10L,
                exitPollIntervalMs = 10L,
                reuseConfirmationDelayMs = 100L,
            )
            val elapsed = System.currentTimeMillis() - started

            assertTrue(launched, "A successful desktop URI handoff should permit reuse tracking")
            assertTrue(elapsed >= 800, "Runner should wait for the reused game to exit (elapsed: ${elapsed}ms)")
            assertTrue(!existingGame.isAlive)
        } finally {
            existingGame.destroyForcibly()
            existingGame.waitFor()
        }
    }

    @Test
    fun `failed launch request does not treat an existing process as a launched game`(@TempDir tempDir: File) = runTest {
        assumeFalse(System.getProperty("os.name", "").lowercase().contains("windows"))
        val gameScript = File(tempDir, "unrelated_game.sh").apply {
            writeText("#!/bin/sh\nsleep \"\$1\"\n")
            setExecutable(true)
        }
        val existingGame = ProcessBuilder(gameScript.absolutePath, "2").start()

        try {
            var existingProcessIds = emptySet<Long>()
            for (attempt in 0 until 50) {
                existingProcessIds = runner.matchingProcessIds(gameScript.name)
                if (existingProcessIds.isNotEmpty()) break
                kotlinx.coroutines.delay(10)
            }
            assertTrue(existingProcessIds.isNotEmpty(), "The existing game process should be detectable")

            val launchRequest = ProcessBuilder("sh", "-c", "exit 1").start()
            val started = System.currentTimeMillis()
            val launched = runner.waitForExternalProcess(
                processName = gameScript.name,
                existingProcessIds = existingProcessIds,
                launchRequest = launchRequest,
                appearanceTimeoutMs = 10_000L,
                pollIntervalMs = 10L,
                exitPollIntervalMs = 10L,
                reuseConfirmationDelayMs = 100L,
            )
            val elapsed = System.currentTimeMillis() - started

            assertTrue(!launched, "A failed launch request must not count as a reused game")
            assertTrue(elapsed < 1_000, "Runner should stop after the failed launch request (elapsed: ${elapsed}ms)")
            assertTrue(existingGame.isAlive, "The unrelated existing process should not be awaited")
        } finally {
            existingGame.destroyForcibly()
            existingGame.waitFor()
        }
    }

    @Test
    fun `isSteamShaderCompiling should detect fossilize_replay active state`(@TempDir tempDir: File) {
        assertEquals(false, runner.isSteamShaderCompiling())

        val isWindows = System.getProperty("os.name", "").lowercase().contains("windows")
        val process = if (isWindows) {
            val pingExe = File(System.getenv("SystemRoot") ?: "C:\\Windows", "System32\\ping.exe")
            val dummyExe = File(tempDir, "fossilize_replay.exe")
            pingExe.copyTo(dummyExe, overwrite = true)
            ProcessBuilder(dummyExe.absolutePath, "127.0.0.1", "-n", "6").start()
        } else {
            val fossilizeScript = File(tempDir, "fossilize_replay").apply {
                writeText("#!/bin/sh\nsleep 5\n")
                setExecutable(true)
            }
            ProcessBuilder(fossilizeScript.absolutePath).start()
        }

        try {
            assertTrue(runner.isSteamShaderCompiling(), "Should detect active fossilize_replay process")
        } finally {
            process.destroyForcibly()
            process.waitFor()
        }
    }
}
