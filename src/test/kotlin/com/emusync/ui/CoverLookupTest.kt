package com.emusync.ui

import com.emusync.model.EmulatorSystem
import com.emusync.model.NativePCGame
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File

class CoverLookupTest {

    @Test
    fun `resolves cover when image is in rom directory with exact name`(@TempDir tempDir: File) {
        val romDir = File(tempDir, "roms").apply { mkdirs() }
        val romFile = File(romDir, "Crash Bandicoot.iso").apply { writeText("dummy") }
        val coverFile = File(romDir, "Crash Bandicoot.png").apply { writeText("img") }

        val system = EmulatorSystem(
            name = "PS1",
            executablePath = "/bin/true",
            romsDirectory = romDir.absolutePath,
            extensions = listOf("iso")
        )
        val item = GameItem(name = "Crash Bandicoot", entry = system, romFile = romFile)

        assertEquals(coverFile.absolutePath, item.coverFile?.absolutePath)
    }

    @Test
    fun `resolves cover from custom coversDirectory`(@TempDir tempDir: File) {
        val romDir = File(tempDir, "roms").apply { mkdirs() }
        val coversDir = File(tempDir, "custom_covers").apply { mkdirs() }
        val romFile = File(romDir, "God of War.iso").apply { writeText("dummy") }
        val coverFile = File(coversDir, "God of War.jpg").apply { writeText("img") }

        val system = EmulatorSystem(
            name = "PS2",
            executablePath = "/bin/true",
            romsDirectory = romDir.absolutePath,
            coversDirectory = coversDir.absolutePath,
            extensions = listOf("iso")
        )
        val item = GameItem(name = "God of War", entry = system, romFile = romFile)

        assertEquals(coverFile.absolutePath, item.coverFile?.absolutePath)
    }

    @Test
    fun `resolves cover from covers subfolder with region tag stripped`(@TempDir tempDir: File) {
        val romDir = File(tempDir, "roms").apply { mkdirs() }
        val coversSubDir = File(romDir, "covers").apply { mkdirs() }
        val romFile = File(romDir, "Midnight Club 3 - DUB Edition Remix (USA).iso").apply { writeText("dummy") }
        val coverFile = File(coversSubDir, "Midnight Club 3 - DUB Edition Remix.webp").apply { writeText("img") }

        val system = EmulatorSystem(
            name = "PS2",
            executablePath = "/bin/true",
            romsDirectory = romDir.absolutePath,
            extensions = listOf("iso")
        )
        val item = GameItem(name = "Midnight Club 3 - DUB Edition Remix (USA)", entry = system, romFile = romFile)

        assertEquals(coverFile.absolutePath, item.coverFile?.absolutePath)
    }

    @Test
    fun `resolves cover for native game with custom coverPath`(@TempDir tempDir: File) {
        val gameDir = File(tempDir, "spelunky").apply { mkdirs() }
        val execFile = File(gameDir, "Spelunky.exe").apply { writeText("dummy") }
        val coverFile = File(tempDir, "my_custom_cover.png").apply { writeText("img") }

        val nativeGame = NativePCGame(
            name = "Spelunky",
            executablePath = execFile.absolutePath,
            coverPath = coverFile.absolutePath
        )
        val item = GameItem(name = "Spelunky", entry = nativeGame)

        assertEquals(coverFile.absolutePath, item.coverFile?.absolutePath)
    }
}
