package com.emusync.drive

import org.junit.jupiter.api.Test
import org.junit.jupiter.api.Assumptions.assumeTrue
import org.junit.jupiter.api.io.TempDir
import com.emusync.model.SaveTargetType
import java.io.File
import java.nio.file.Files
import java.nio.file.Path
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

class SaveRestorePathsTest {
    @Test
    fun `nested restore targets retain their exact remote keys`(@TempDir tempDir: File) {
        val targets = resolveCloudRestoreTargets(tempDir.path, listOf("profiles/one/save.dat", "profiles/two/save.dat"))

        assertEquals(listOf("profiles/one/save.dat", "profiles/two/save.dat"), targets.map { it.cloudPath })
        assertEquals(listOf(File(tempDir, "profiles/one/save.dat").path, File(tempDir, "profiles/two/save.dat").path), targets.map { it.path })
        assertTrue(targets.all { it.type == SaveTargetType.FILE })
    }

    @Test
    fun `different remote names resolving to one local file are rejected before restoration`(@TempDir tempDir: File) {
        val targets = resolveCloudRestoreTargets(tempDir.path, listOf("slot/save.dat", "slot\\save.dat"))

        kotlin.test.assertFailsWith<SaveTargetMappingException> { validateSaveTargetMappings(targets, emptyList()) }
        assertTrue(tempDir.list().isNullOrEmpty())
    }

    @Test
    fun `remote file cannot also become a parent directory`(@TempDir tempDir: File) {
        val targets = resolveCloudRestoreTargets(tempDir.path, listOf("slot", "slot/save.dat"))

        kotlin.test.assertFailsWith<SaveTargetMappingException> { validateSaveTargetMappings(targets, emptyList()) }
        assertTrue(tempDir.list().isNullOrEmpty())
    }

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
            "C:outside.sav",
            "slot\u0000/save.sav",
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

    @Test
    fun `restore paths reject a nested symlink that points outside the selected folder`(@TempDir tempDir: File) {
        val selectedFolder = File(tempDir, "selected").apply { mkdirs() }
        val outsideFolder = File(tempDir, "outside").apply { mkdirs() }
        val outsideSave = File(outsideFolder, "save.dat").apply { writeText("keep me") }

        val symlinkCreated = runCatching {
            Files.createSymbolicLink(Path.of(selectedFolder.path, "slot"), outsideFolder.toPath())
        }.isSuccess
        assumeTrue(symlinkCreated, "This platform does not permit creating symbolic links")

        assertNull(resolveRemoteFile(selectedFolder, "slot/save.dat"))
        assertEquals(emptyList(), resolveCloudRestorePaths(selectedFolder.absolutePath, listOf("slot/save.dat")))
        assertEquals("keep me", outsideSave.readText())
        assertTrue(Files.isSymbolicLink(Path.of(selectedFolder.path, "slot")))
    }
}
