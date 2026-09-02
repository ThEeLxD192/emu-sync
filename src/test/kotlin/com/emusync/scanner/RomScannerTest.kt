package com.emusync.scanner

import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class RomScannerTest {

    @Test
    fun `should scan ROM files recursively and ignore non-ROM files`(@TempDir tempDir: File) = runTest {
        // Create test structure
        val filesToCreate = listOf(
            "pokemon_emerald.gba",
            "metroid_fusion.gba",
            "zelda_minish_cap.GBA",  // uppercase extension should match
            "advance_wars.gbc",
            "readme.txt",             // excluded
            "cover.png",              // excluded
            "subfolder/hidden_game.gba" // recursive match
        )

        filesToCreate.forEach { path ->
            val file = File(tempDir, path)
            file.parentFile.mkdirs()
            file.createNewFile()
        }

        val roms = scanRoms(tempDir.absolutePath, listOf("gba", "gbc"))

        assertEquals(5, roms.size)
        assertTrue(roms.all { it.extension in setOf("gba", "gbc") })
        assertTrue(roms.none { it.name == "readme" || it.name == "cover" })
        assertTrue(roms.any { it.name == "hidden_game" })
        
        // Should be sorted alphabetically
        val names = roms.map { it.name.lowercase() }
        assertEquals(names.sorted(), names)
    }

    @Test
    fun `should return empty list when scanning nonexistent directory`() = runTest {
        val result = scanRoms("/nonexistent/directory/path/12345", listOf("gba"))
        assertTrue(result.isEmpty())
    }

    @Test
    fun `should handle empty extensions list gracefully`(@TempDir tempDir: File) = runTest {
        File(tempDir, "game.gba").createNewFile()
        val result = scanRoms(tempDir.absolutePath, emptyList())
        assertTrue(result.isEmpty())
    }
}
