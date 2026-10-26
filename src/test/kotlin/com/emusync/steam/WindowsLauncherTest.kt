package com.emusync.steam

import com.emusync.model.EmulatorSystem
import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import org.junit.jupiter.api.Assumptions.assumeTrue
import java.io.File
import java.util.Base64
import java.util.concurrent.TimeUnit
import javax.tools.ToolProvider
import kotlin.test.*

class WindowsLauncherTest {
    @Test
    fun `wrapper keeps paths and arguments outside batch source`(@TempDir directory: File) {
        val root = File(directory, "launchers & ' %PATH% !value!")
        val entry = emulator(directory).copy(arguments = listOf("a\" & echo bad & \"b", "%PATH%", "!value!", "中文"))
        val manager = SteamShortcutManager(customLauncherDir = root)
        val batch = manager.createWindowsWrapperScript(entry)
        val source = batch.readText()
        assertTrue(source.contains("DisableDelayedExpansion"))
        assertTrue(source.contains("-EncodedCommand "))
        assertFalse(source.contains(entry.executablePath))
        assertFalse(source.contains(root.path))
        assertFalse(source.contains("echo bad"))
        val config = Json.decodeFromString<WindowsLaunchConfiguration>(File(root, "launch_${manager.launcherKey(entry.name)}.json").readText())
        assertEquals(entry.arguments, config.arguments)
        assertEquals(manager.getSignalFile(entry).absolutePath, config.signalFile)
        val command = WindowsSteamLauncher(root).encodedCommand(manager.launcherKey(entry.name))
        val decoded = Base64.getDecoder().decode(command).toString(Charsets.UTF_16LE)
        assertFalse(decoded.contains(root.path))
        assertTrue(decoded.contains("FromBase64String"))
    }

    @Test
    fun `PowerShell launcher preserves empty quotes slashes metacharacters and exit code`(@TempDir directory: File) {
        val powershell = powershell()
        val source = File(directory, "ArgsRecorder.java").apply { writeText("""
            import java.nio.file.*;
            import java.util.*;
            public class ArgsRecorder {
                public static void main(String[] args) throws Exception {
                    Thread.sleep(100);
                    Files.writeString(Path.of(args[0]), String.join("\u0000", Arrays.copyOfRange(args, 1, args.length)));
                    System.exit(37);
                }
            }
        """.trimIndent()) }
        assertEquals(0, ToolProvider.getSystemJavaCompiler().run(null, null, null, source.path))
        val output = File(directory, "arguments.bin")
        val root = File(directory, "launchers & ' %PATH% !value!")
        val java = File(System.getProperty("java.home"), "bin/${if (isWindows()) "java.exe" else "java"}")
        val payload = listOf("", "with spaces", "C:\\a path\\", "a\\\"b", "a\"b", "%PATH%", "!value!", "a&b|c<d>e", "\$(literal)", "中文 ' \"", "--rom={ROM}", "last")
        val entry = emulator(directory).copy(executablePath = java.path,
            arguments = listOf("-cp", directory.path, "ArgsRecorder", output.path) + payload)
        val manager = SteamShortcutManager(customLauncherDir = root)
        val rom = File(directory, "ROM & ' %TEST% !name!.rom").apply { writeText("ROM") }
        manager.prepareLaunch(entry, rom)
        manager.createWindowsWrapperScript(entry)
        val command = WindowsSteamLauncher(root).encodedCommand(manager.launcherKey(entry.name))
        val process = ProcessBuilder(powershell, "-NoLogo", "-NoProfile", "-NonInteractive", "-EncodedCommand", command)
            .redirectErrorStream(true).start()
        assertTrue(process.waitFor(20, TimeUnit.SECONDS), "PowerShell launch timed out")
        val log = process.inputStream.bufferedReader().readText()
        assertEquals(37, process.exitValue(), log)
        assertEquals(payload.map { it.replace("{ROM}", rom.path) }, output.readText().split('\u0000'))
    }

    @Test
    fun `missing ROM and batch emulator fail without launching`(@TempDir directory: File) {
        val powershell = powershell()
        val manager = SteamShortcutManager(customLauncherDir = directory)
        for (executable in listOf("missing.exe", "unsafe.cmd")) {
            val entry = emulator(directory).copy(executablePath = executable)
            manager.createWindowsWrapperScript(entry)
            val process = ProcessBuilder(powershell, "-NoProfile", "-NonInteractive", "-EncodedCommand",
                WindowsSteamLauncher(directory).encodedCommand(manager.launcherKey(entry.name))).redirectErrorStream(true).start()
            assertTrue(process.waitFor(10, TimeUnit.SECONDS))
            assertEquals(1, process.exitValue())
        }
    }

    private fun emulator(directory: File) = EmulatorSystem("Windows", "C:\\test\\emulator.exe", romsDirectory = directory.path,
        extensions = listOf("rom"), fullscreenArgs = "")
    private fun isWindows() = System.getProperty("os.name").lowercase().contains("windows")
    private fun powershell(): String {
        val executable = System.getenv("EMUSYNC_TEST_PWSH") ?: if (isWindows()) "powershell.exe" else
            System.getenv("PATH").orEmpty().split(File.pathSeparator).map { File(it, "pwsh") }
                .firstOrNull { it.isFile && it.canExecute() }?.path
        assumeTrue(executable != null, "PowerShell integration requires Windows PowerShell or EMUSYNC_TEST_PWSH")
        return executable!!
    }
}
