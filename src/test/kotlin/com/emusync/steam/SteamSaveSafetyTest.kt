package com.emusync.steam

import com.emusync.model.EmulatorSystem
import com.emusync.model.NativePCGame
import com.emusync.util.shellArgument
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import org.junit.jupiter.api.Assumptions.assumeFalse
import java.io.File
import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlin.test.*

class SteamSaveSafetyTest {
    @Test
    fun `Linux wrapper preserves literal arguments quoted flags and embedded ROM paths`(@TempDir directory: File) {
        assumeFalse(System.getProperty("os.name").lowercase().contains("windows"))
        val output = File(directory, "args.bin")
        val marker = File(directory, "must-not-be-created")
        val emulator = File(directory, "emulator \$cash").apply {
            writeText("#!/bin/bash\nprintf '%s\\0' \"\$@\" > ${shellArgument(output.path)}\n")
            setExecutable(true)
        }
        val rom = File(directory, "game folder.rom").apply { mkdirs() }
        val manager = SteamShortcutManager(customLauncherDir = File(directory, "launchers \$literal"))
        val arguments = listOf("", "\$HOME", "\$(touch ${shellArgument(marker.path)})", "`echo injected`", "literal \"quotes\"", "--load={ROM}")
        val entry = EmulatorSystem("Safe", emulator.path, romsDirectory = directory.path, extensions = listOf("rom"),
            arguments = arguments, fullscreenArgs = "--label \"value with spaces\"")
        manager.prepareLaunch(entry, rom)
        val script = manager.createLinuxWrapperScript(entry)
        val process = ProcessBuilder("bash", script.path).redirectErrorStream(true).start()
        assertTrue(process.waitFor(5, TimeUnit.SECONDS), "Wrapper did not exit")
        assertEquals(0, process.exitValue(), process.inputStream.bufferedReader().readText())
        val actual = output.readBytes().decodeToString().split('\u0000').dropLast(1)
        assertEquals(listOf("--label", "value with spaces") + arguments.map { it.replace("{ROM}", rom.path) }, actual)
        assertFalse(marker.exists())
    }

    @Test
    fun `native re-registration updates launch paths while preserving Steam id and preferences`(@TempDir directory: File) {
        val file = File(directory, "shortcuts.vdf")
        val shortcut = SteamShortcut(1234, "[EmuSync] Old", "old.exe", "old-dir", allowOverlay = false,
            tags = mapOf("0" to "Favourites"), extraFields = mapOf("FutureField" to mapOf("nested" to "keep me")))
        BinaryVdf.write(file, listOf(shortcut))
        val manager = object : SteamShortcutManager(customLauncherDir = File(directory, "launchers")) {
            override fun findShortcutsFile() = file
        }
        val updated = manager.registerEntry(NativePCGame("New", "/games/new.exe", steamAppId = 1234))
        assertEquals(1234, updated.appId)
        assertEquals("\"/games/new.exe\"", updated.exe)
        assertEquals("[EmuSync] New", updated.appName)
        assertFalse(updated.allowOverlay)
        assertEquals(shortcut.tags, updated.tags)
        assertEquals(shortcut.extraFields, BinaryVdf.read(file).single().extraFields)
    }

    @Test
    fun `interrupted VDF replacement leaves the complete previous file`(@TempDir directory: File) {
        val file = File(directory, "shortcuts.vdf")
        val original = SteamShortcut(1, "Original", "game", ".")
        BinaryVdf.write(file, listOf(original))
        val bytes = file.readBytes()
        assertFailsWith<IOException> {
            BinaryVdf.write(file, listOf(original.copy(appName = "New"))) { throw IOException("disk error") }
        }
        assertContentEquals(bytes, file.readBytes())
        assertEquals(listOf(file.name), directory.listFiles()!!.map { it.name })
    }

    @Test
    fun `truncated VDF values are rejected instead of being rewritten as valid entries`(@TempDir directory: File) {
        val file = File(directory, "shortcuts.vdf")
        BinaryVdf.write(file, listOf(SteamShortcut(1, "Original", "game", ".")))
        val bytes = file.readBytes()
        val intStart = bytes.decodeToString().indexOf("appid") + "appid".length + 1
        for (length in listOf(1, 9, intStart + 2, bytes.size - 3)) {
            val truncated = bytes.copyOf(length)
            file.writeBytes(truncated)
            assertFails { BinaryVdf.read(file) }
            assertContentEquals(truncated, file.readBytes())
        }
    }

    @Test
    fun `NUL in VDF fields is rejected before touching the original file`(@TempDir directory: File) {
        val file = File(directory, "shortcuts.vdf")
        val original = SteamShortcut(1, "Original", "game", ".")
        BinaryVdf.write(file, listOf(original))
        val bytes = file.readBytes()
        assertFailsWith<IllegalArgumentException> { BinaryVdf.write(file, listOf(original.copy(appName = "bad\u0000value"))) }
        assertContentEquals(bytes, file.readBytes())
    }
}
