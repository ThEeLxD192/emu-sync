package com.emusync.drive

import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertNull

class SaveRestorePathsTest {
    @Test
    fun `restore paths are empty until a destination is selected`() {
        assertEquals(emptyList(), resolveCloudRestorePaths("", listOf("save.mcd")))
    }

    @Test
    fun `restore folder resolves to one explicit path per cloud file`(@TempDir tempDir: File) {
        val paths = resolveCloudRestorePaths(
            destinationFolder = tempDir.absolutePath,
            cloudFileNames = listOf("Black_1.mcd", "Black_2.mcd"),
        )

        assertEquals(
            listOf(
                File(tempDir, "Black_1.mcd").absolutePath,
                File(tempDir, "Black_2.mcd").absolutePath,
            ),
            paths,
        )
    }

    @Test
    fun `restore folder ignores names that escape the selected folder`(@TempDir tempDir: File) {
        val paths = resolveCloudRestorePaths(
            destinationFolder = tempDir.absolutePath,
            cloudFileNames = listOf("safe.mcd", "../outside.mcd"),
        )

        assertEquals(listOf(File(tempDir, "safe.mcd").absolutePath), paths)
    }

    @Test
    fun `remote paths reject traversal absolute paths and empty segments`(@TempDir tempDir: File) {
        val invalidNames = listOf(
            "../outside.sav",
            "..\\outside.sav",
            "folder/../../outside.sav",
            "/tmp/outside.sav",
            "\\\\server\\share\\outside.sav",
            "C:\\outside.sav",
            "folder//save.sav",
            "folder\\\\save.sav",
            "./save.sav",
            "folder/./save.sav",
        )

        invalidNames.forEach { name ->
            assertNull(resolveRemoteFile(tempDir, name), "Expected '$name' to be rejected")
        }
    }

    @Test
    fun `remote paths accept nested names with either separator`(@TempDir tempDir: File) {
        assertEquals(
            File(tempDir, "slot/one/save.dat").absoluteFile.normalize(),
            resolveRemoteFile(tempDir, "slot/one/save.dat")?.absoluteFile?.normalize(),
        )
        assertEquals(
            File(tempDir, "slot/one/save.dat").absoluteFile.normalize(),
            resolveRemoteFile(tempDir, "slot\\one\\save.dat")?.absoluteFile?.normalize(),
        )
    }
}
