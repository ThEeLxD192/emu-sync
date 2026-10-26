package com.emusync.steam

import com.emusync.model.EmulatorSystem
import com.emusync.util.shellArgument
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import org.junit.jupiter.api.Assumptions.assumeFalse
import java.io.File
import java.util.concurrent.TimeUnit
import kotlin.test.*

class LauncherIsolationTest {
    @Test
    fun `normalized names Unicode and case variations have different stable paths`(@TempDir directory: File) {
        val manager = SteamShortcutManager(customLauncherDir = directory)
        val names = listOf("Foo-Bar", "Foo Bar", "foo bar", "Foo_Bar", "中文", "日本語")
        val entries = names.map { emulator(it, directory) }
        val keys = entries.map { manager.launcherKey(it.name) }
        assertEquals(names.size, keys.toSet().size)
        assertEquals(keys, names.map { manager.launcherKey(it) })
        assertEquals(names.size, entries.map { manager.getSignalFile(it).path.lowercase() }.toSet().size)
        entries.forEach { manager.createWrapperScript(it) }
        assertEquals(names.size, directory.listFiles()!!.count { it.extension in setOf("sh", "bat") })
    }

    @Test
    fun `migration retains Steam ids settings and a working legacy shortcut`(@TempDir directory: File) {
        assumeFalse(isWindows())
        val manager = manager(directory)
        val output = File(directory, "output")
        val executable = recorder(directory, output)
        val entry = emulator("Foo-Bar", directory).copy(executablePath = executable.path)
        val rom = File(directory, "previous.rom").apply { writeText("ROM") }
        val legacy = File(directory, "launch_foo_bar.sh").apply { writeText("#!/bin/bash\nexit 99\n"); setExecutable(true) }
        File(directory, ".current_rom_foo_bar").writeText(rom.path)
        val shortcut = SteamShortcut(123, "[EmuSync] ${entry.name}", "\"${legacy.path}\"", directory.path,
            allowOverlay = false, tags = mapOf("0" to "Keep"))
        BinaryVdf.write(File(directory, "shortcuts.vdf"), listOf(shortcut))
        val migrated = manager.migrateLegacyLaunchers(listOf(entry)).single()
        assertEquals(123, migrated.appId)
        assertFalse(migrated.allowOverlay)
        assertEquals(shortcut.tags, migrated.tags)
        assertNotEquals(shortcut.exe, migrated.exe)
        assertEquals(rom.path, manager.getSignalFile(entry).readText())
        assertEquals(0, runScript(legacy))
        assertEquals(rom.path, output.readText())
        assertTrue(File(directory, "launch_foo_bar.sh.legacy.bak").readText().contains("exit 99"))
        assertEquals(migrated, manager.migrateLegacyLaunchers(listOf(entry)).single())
    }

    @Test
    fun `ambiguous old alias refuses to launch but new wrappers keep separate ROM signals`(@TempDir directory: File) {
        assumeFalse(isWindows())
        val manager = manager(directory)
        val output = File(directory, "output")
        val executable = recorder(directory, output)
        val entries = listOf("Foo-Bar", "Foo Bar").map { emulator(it, directory).copy(executablePath = executable.path) }
        val legacy = File(directory, "launch_foo_bar.sh").apply { writeText("old shared script"); setExecutable(true) }
        File(directory, ".current_rom_foo_bar").writeText("ambiguous old ROM")
        BinaryVdf.write(File(directory, "shortcuts.vdf"), entries.mapIndexed { index, entry ->
            SteamShortcut(index + 1, "[EmuSync] ${entry.name}", "\"${legacy.path}\"", directory.path)
        })
        val migrated = manager.migrateLegacyLaunchers(entries)
        assertEquals(listOf(1, 2), migrated.map { it.appId })
        assertEquals(2, migrated.map { it.exe }.toSet().size)
        assertEquals(1, runScript(legacy))
        assertFalse(output.exists())
        assertTrue(entries.none { manager.getSignalFile(it).exists() })
        val roms = entries.indices.map { File(directory, "$it.rom").apply { writeText("ROM") } }
        entries.zip(roms).forEach { (entry, rom) -> manager.prepareLaunch(entry, rom) }
        entries.zip(roms).forEach { (entry, rom) ->
            assertEquals(0, runScript(manager.createLinuxWrapperScript(entry)))
            assertEquals(rom.path, output.readText())
        }
        assertEquals("ambiguous old ROM", File(directory, ".current_rom_foo_bar").readText())
    }

    @Test
    fun `renaming a hashed launcher keeps the shortcut already loaded by Steam working`(@TempDir directory: File) {
        assumeFalse(isWindows())
        val manager = manager(directory)
        val output = File(directory, "output")
        val executable = recorder(directory, output)
        val original = emulator("Original", directory).copy(executablePath = executable.path)
        val shortcut = manager.registerEntry(original)
        val oldScript = File(shortcut.exe.removeSurrounding("\""))
        val renamed = original.copy(name = "Renamed", steamAppId = shortcut.appId)
        val updated = manager.registerEntry(renamed)
        val rom = File(directory, "new.rom").apply { writeText("ROM") }
        manager.prepareLaunch(renamed, rom)
        assertEquals(shortcut.appId, updated.appId)
        assertNotEquals(shortcut.exe, updated.exe)
        assertEquals(0, runScript(oldScript))
        assertEquals(rom.path, output.readText())
    }

    private fun manager(directory: File) = object : SteamShortcutManager(customLauncherDir = directory) {
        override fun findShortcutsFile() = File(directory, "shortcuts.vdf")
        override fun isWindows() = false
    }
    private fun emulator(name: String, directory: File) = EmulatorSystem(name, "echo", romsDirectory = directory.path,
        extensions = listOf("rom"), fullscreenArgs = "")
    private fun recorder(directory: File, output: File) = File(directory, "record.sh").apply {
        writeText("#!/bin/bash\nprintf '%s' \"\$1\" > ${shellArgument(output.path)}\n")
        setExecutable(true)
    }
    private fun runScript(file: File): Int {
        val process = ProcessBuilder("bash", file.path).redirectOutput(ProcessBuilder.Redirect.DISCARD)
            .redirectError(ProcessBuilder.Redirect.DISCARD).start()
        assertTrue(process.waitFor(5, TimeUnit.SECONDS))
        return process.exitValue()
    }
    private fun isWindows() = System.getProperty("os.name").lowercase().contains("windows")
}
