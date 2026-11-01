package com.emusync.ui.dialogs

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetAccess
import com.emusync.domain.library.SaveTargetType
import com.emusync.local.saves.LocalSaveTargets
import com.emusync.ui.library.form.SaveTargetFormState
import com.emusync.ui.library.form.readFormDraft
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class SaveTargetFormStateTest {
    @Test
    fun `late type discovery cannot change the type of a newer path`() = runTest {
        val access =
            object : SaveTargetAccess by LocalSaveTargets() {
                override fun typeOf(path: String) = SaveTargetType.DIRECTORY
            }
        val form = SaveTargetFormState(emptyList(), access)
        form.updatePath(0, "old")
        val discovery =
            async(start = CoroutineStart.UNDISPATCHED) {
                form.refreshType(0, "old", StandardTestDispatcher(testScheduler))
            }
        form.updatePath(0, "new")
        discovery.await()
        assertEquals(listOf("new"), form.savePaths)
        assertEquals(listOf(SaveTargetType.FILE), form.saveTypes)
    }

    @Test
    fun `saving resolves a coherent draft even when fields change during IO dispatch`() = runTest {
        var title by mutableStateOf("before")
        var path by mutableStateOf("before.sav")
        var error by mutableStateOf<String?>(null)
        val saving =
            async(start = CoroutineStart.UNDISPATCHED) {
                readFormDraft(StandardTestDispatcher(testScheduler)) {
                    error = "Validation failed"
                    title to path
                }
            }
        title = "after"
        path = "after.sav"
        assertEquals("before" to "before.sav", saving.await())
        assertEquals("after", title)
        assertEquals("Validation failed", error)
    }

    @Test
    fun `removing a row keeps the remaining path type and cloud name together`(
        @TempDir directory: File
    ) {
        val first = File(directory, "first.sav").path
        val second = File(directory, "second").path
        val form =
            SaveTargetFormState(
                listOf(SaveTarget(first, SaveTargetType.FILE, "first.sav")),
                com.emusync.local.saves.LocalSaveTargets(),
            )
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
    ) = runTest {
        val form = SaveTargetFormState(emptyList(), com.emusync.local.saves.LocalSaveTargets())
        form.refreshType(0, "")
        assertEquals(SaveTargetType.FILE, form.saveTypes.single())
        form.updateType(0, SaveTargetType.DIRECTORY)
        form.updatePath(0, File(directory, "future-save").path)
        assertEquals(SaveTargetType.DIRECTORY, form.saveTypes.single())

        val file = File(directory, "save.dat").apply { writeText("SAVE") }
        form.updatePath(0, file.path)
        form.refreshType(0, file.path)
        assertEquals(SaveTargetType.FILE, form.saveTypes.single())
        form.updateType(0, SaveTargetType.DIRECTORY)
        assertEquals(SaveTargetType.FILE, form.buildTargets().single().type)

        form.updatePath(0, directory.path)
        form.refreshType(0, directory.path)
        assertEquals(SaveTargetType.DIRECTORY, form.saveTypes.single())
    }

    @Test
    fun `blank rows are excluded and target values are trimmed`(@TempDir directory: File) {
        val path = File(directory, "future.sav").path
        val form = SaveTargetFormState(emptyList(), com.emusync.local.saves.LocalSaveTargets())
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
        val form = SaveTargetFormState(emptyList(), com.emusync.local.saves.LocalSaveTargets())
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
