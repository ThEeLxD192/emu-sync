package com.emusync.scanner

import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class RomScannerTest {

    @Test
    fun `should scan ROM files recursively and ignore non-ROM files`(@TempDir tempDir: File) =
        runTest {
            // Create test structure
            val filesToCreate =
                listOf(
                    "pokemon_emerald.gba",
                    "metroid_fusion.gba",
                    "zelda_minish_cap.GBA", // uppercase extension should match
                    "advance_wars.gbc",
                    "readme.txt", // excluded
                    "cover.png", // excluded
                    "subfolder/hidden_game.gba", // recursive match
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

    @Test
    fun `symlink cycles and aliases do not duplicate games or recurse indefinitely`(
        @TempDir directory: File
    ) = runTest {
        val games = File(directory, "games").apply { mkdirs() }
        File(games, "game.gba").writeText("ROM")
        try {
            java.nio.file.Files.createSymbolicLink(File(games, "loop").toPath(), directory.toPath())
            java.nio.file.Files.createSymbolicLink(
                File(directory, "alias").toPath(),
                games.toPath(),
            )
        } catch (failure: Exception) {
            org.junit.jupiter.api.Assumptions.assumeTrue(
                false,
                "Symbolic links are unavailable: ${failure.message}",
            )
        }
        val roms = scanRoms(directory.path, listOf("gba"))
        assertEquals(1, roms.size)
        assertEquals("game", roms.single().name)
    }

    @Test
    fun `extensions accept whitespace dots and mixed case`(@TempDir directory: File) = runTest {
        File(directory, "game.GBA").writeText("ROM")
        assertEquals(1, scanRoms(directory.path, listOf(" .gBa ")).size)
    }

    @Test
    fun `signature files identify complete games and exclude their individual ROMs`(
        @TempDir directory: File
    ) = runTest {
        val gameFolders =
            listOf("PS3_DISC", "EBOOT", "APP", "MAIN").map { signature ->
                File(directory, "Game-$signature").apply {
                    mkdirs()
                    File(this, "ordinary.bin").writeText("ROM")
                    File(this, "nested").mkdirs()
                    File(this, "nested/hidden.bin").writeText("ROM")
                    File(this, "$signature.BIN").writeText("Signature")
                }
            }
        val standalone = File(directory, "Standalone.bin").apply { writeText("ROM") }

        val roms = scanRoms(directory.path, listOf("bin"))

        assertEquals((gameFolders + standalone).toSet(), roms.map { it.file }.toSet())
        assertEquals(5, roms.size)
        assertTrue(roms.all { it.extension == "bin" })
        assertEquals(
            (gameFolders.map { it.name } + "Standalone").sortedBy { it.lowercase() },
            roms.map { it.name },
        )
    }

    @Test
    fun `signature with an excluded extension does not hide matching ROMs or subfolders`(
        @TempDir directory: File
    ) = runTest {
        File(directory, "eboot.txt").writeText("Not an allowed ROM")
        val ordinary = File(directory, "ordinary.gba").apply { writeText("ROM") }
        val nested =
            File(directory, "nested/game.gba").apply {
                parentFile.mkdirs()
                writeText("ROM")
            }

        val roms = scanRoms(directory.path, listOf("gba"))

        assertEquals(setOf(ordinary, nested), roms.map { it.file }.toSet())
    }

    @Test
    fun `a regular file cannot be scanned as a directory`(@TempDir directory: File) = runTest {
        val file = File(directory, "game.gba").apply { writeText("ROM") }

        assertTrue(scanRoms(file.path, listOf("gba")).isEmpty())
    }

    @Test
    fun `extensions that normalize to empty do not match files without an extension`(
        @TempDir directory: File
    ) = runTest {
        File(directory, "game").writeText("ROM")

        assertTrue(scanRoms(directory.path, listOf("", " ", " . ")).isEmpty())
    }
}
