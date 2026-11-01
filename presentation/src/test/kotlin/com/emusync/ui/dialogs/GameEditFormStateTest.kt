package com.emusync.ui.dialogs

import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetType
import com.emusync.ui.library.form.GameEditFormState
import com.emusync.ui.testGameItem
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class GameEditFormStateTest {
    @Test
    fun `game drafts do not import emulator wide fallback saves`(@TempDir directory: File) {
        val entry =
            emulator()
                .copy(
                    saveTargets =
                        listOf(SaveTarget(File(directory, "global.dat").path, SaveTargetType.FILE))
                )
        val game = testGameItem("Game", entry, File(directory, "game.rom"))
        val form = GameEditFormState(game, com.emusync.local.saves.LocalSaveTargets())

        assertEquals(listOf(""), form.saves.savePaths)
        assertTrue(form.result(game, emptyList()).targets.isEmpty())
    }

    @Test
    fun `blank title and cover overrides become null while existing game targets keep cloud names`(
        @TempDir directory: File
    ) {
        val targets =
            listOf(SaveTarget(File(directory, "save.dat").path, SaveTargetType.FILE, "slot.dat"))
        val game =
            testGameItem(
                "Game",
                emulator().copy(saveTargetsByRom = mapOf("game.rom" to targets)),
                File(directory, "game.rom"),
            )
        val form =
            GameEditFormState(game, com.emusync.local.saves.LocalSaveTargets()).apply {
                customTitle = " "
                coverPath = " "
            }

        val result = form.result(game, emptyList())

        assertNull(result.title)
        assertNull(result.coverPath)
        assertEquals(targets, result.targets)
    }

    @Test
    fun `individual cloud restore keeps exact remote names and avoids whole directory syncing`(
        @TempDir directory: File
    ) {
        val game = testGameItem("Game", emulator(), File(directory, "game.rom"))
        val form = GameEditFormState(game, com.emusync.local.saves.LocalSaveTargets())
        val names = listOf("save.dat", "slot/options.ini")
        assertFalse(form.canSave(names, form.restorePaths(names)))
        val destination = File(directory, "restores")
        form.restoreDestinationFolder = " ${destination.path} "

        val targets = form.result(game, names).targets

        assertTrue(form.canSave(names, form.restorePaths(names)))
        assertEquals(names, targets.map { it.cloudPath })
        assertEquals(names.map { File(destination, it).absolutePath }, targets.map { it.path })
        assertTrue(targets.all { it.type == SaveTargetType.FILE })
        assertFalse(destination.exists())
    }

    @Test
    fun `whole folder restore produces one directory target`(@TempDir directory: File) {
        val game = testGameItem("Game", emulator())
        val form =
            GameEditFormState(game, com.emusync.local.saves.LocalSaveTargets()).apply {
                restoreDestinationFolder = directory.path
                restoreWholeFolder = true
            }
        val names = listOf("slot/save.dat", "settings.ini")

        assertEquals(listOf(directory.path), form.restorePaths(names))
        assertEquals(
            listOf(SaveTarget(directory.path, SaveTargetType.DIRECTORY)),
            form.result(game, names).targets,
        )
    }

    @Test
    fun `unsafe restore names cannot enable saving outside the destination`(
        @TempDir directory: File
    ) {
        val game = testGameItem("Game", emulator())
        val form =
            GameEditFormState(game, com.emusync.local.saves.LocalSaveTargets()).apply {
                restoreDestinationFolder = directory.path
            }
        val names = listOf("../escape.dat", "/absolute.dat")

        assertTrue(form.restorePaths(names).isEmpty())
        assertFalse(form.canSave(names, form.restorePaths(names)))
        assertTrue(form.result(game, names).targets.isEmpty())
    }

    @Test
    fun `invalid edited cloud names disable saving even without cloud restore`() {
        val game = testGameItem("Game", emulator())
        val form = GameEditFormState(game, com.emusync.local.saves.LocalSaveTargets())
        form.saves.updateCloudPath(0, "../escape.dat")

        assertFalse(form.canSave(emptyList(), emptyList()))
    }

    private fun emulator() =
        EmulatorSystem("Emulator", "/emulator", romsDirectory = "/roms", extensions = listOf("rom"))
}
