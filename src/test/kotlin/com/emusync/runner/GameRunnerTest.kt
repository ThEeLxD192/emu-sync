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
}
