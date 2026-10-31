package com.emusync.ui.dialogs

import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetType
import com.emusync.ui.library.form.SaveTargetFormState
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class SaveTargetFormStateTest {
    @Test
    fun `removing a row keeps the remaining path type and cloud name together`(
        @TempDir directory: File
    ) {
        val first = File(directory, "first.sav").path
        val second = File(directory, "second").path
        val form = SaveTargetFormState(listOf(SaveTarget(first, SaveTargetType.FILE, "first.sav")))
        form.addPath()
        form.updatePath(1, second)
        form.updateType(1, SaveTargetType.DIRECTORY)
        form.updateCloudPath(1, "second-prefix")

        form.removePath(0)

        assertEquals(listOf(second), form.savePaths)
        assertEquals(listOf(SaveTargetType.DIRECTORY), form.saveTypes)
        assertEquals(listOf("second-prefix"), form.saveCloudPaths)
        assertEquals(
            listOf(SaveTarget(second, SaveTargetType.DIRECTORY, "second-prefix")),
            form.buildTargets(),
        )
    }

    @Test
    fun `physical paths determine their type while missing paths retain the selected type`(
        @TempDir directory: File
    ) {
        val form = SaveTargetFormState(emptyList())
        form.updateType(0, SaveTargetType.DIRECTORY)
        form.updatePath(0, File(directory, "future-save").path)
        assertEquals(SaveTargetType.DIRECTORY, form.saveTypes.single())

        val file = File(directory, "save.dat").apply { writeText("SAVE") }
        form.updatePath(0, file.path)
        assertEquals(SaveTargetType.FILE, form.saveTypes.single())
        form.updateType(0, SaveTargetType.DIRECTORY)
        assertEquals(SaveTargetType.FILE, form.buildTargets().single().type)

        form.updatePath(0, directory.path)
        assertEquals(SaveTargetType.DIRECTORY, form.saveTypes.single())
    }

    @Test
    fun `blank rows are excluded and target values are trimmed`(@TempDir directory: File) {
        val path = File(directory, "future.sav").path
        val form = SaveTargetFormState(emptyList())
        form.updatePath(0, "  $path  ")
        form.updateCloudPath(0, "  slot/save.sav  ")
        form.addPath()
        form.updatePath(1, "   ")

        assertEquals(
            listOf(SaveTarget(path, SaveTargetType.FILE, "slot/save.sav")),
            form.buildTargets(),
        )
    }

    @Test
    fun `cloud path validation accepts relative names and rejects traversal or absolute paths`() {
        val form = SaveTargetFormState(emptyList())
        for (path in listOf("", " ", "slot/save.dat", "profile")) {
            form.updateCloudPath(0, path)
            assertTrue(form.cloudPathsAreValid, path)
        }
        for (path in listOf("../escape", "/absolute/save", "C:\\save.dat", "slot/../save.dat")) {
            form.updateCloudPath(0, path)
            assertFalse(form.cloudPathsAreValid, path)
        }
    }
}
