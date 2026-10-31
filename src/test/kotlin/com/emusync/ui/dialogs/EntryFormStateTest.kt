package com.emusync.ui.dialogs

import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetType
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class EntryFormStateTest {
    @Test
    fun `new emulator trims fields and preserves quoted arguments and extension order`() {
        val form =
            emulatorForm().apply {
                name = "  Emulator  "
                executablePath = "  /games/emulator  "
                arguments = " --profile \"Two Words\" {ROM} "
                extensions = " gba, GBC zip "
                group = " Handhelds "
                fullscreenArgs = " --fullscreen "
            }

        val entry = assertIs<EmulatorSystem>(form.buildEntry(null))

        assertEquals("Emulator", entry.name)
        assertEquals("/games/emulator", entry.executablePath)
        assertEquals(listOf("--profile", "Two Words", "{ROM}"), entry.arguments)
        assertEquals(listOf("gba", "GBC", "zip"), entry.extensions)
        assertEquals("Handhelds", entry.group)
        assertEquals("--fullscreen", entry.fullscreenArgs)
        assertEquals(emptyList(), entry.saveTargets)
    }

    @Test
    fun `native games require a save target and trim optional process and cover fields`(
        @TempDir directory: File
    ) {
        val form =
            emulatorForm().apply {
                entryType = EntryType.NATIVE
                applyTypeDefaults()
                waitForProcess = "  game.exe  "
                coverPath = "  /images/game.png  "
            }
        assertNull(form.buildEntry(null))
        form.saves.updatePath(0, "  ${File(directory, "save.dat").path}  ")

        val entry = assertIs<NativePCGame>(form.buildEntry(null))

        assertEquals(listOf(File(directory, "save.dat").path), entry.savePaths)
        assertEquals("game.exe", entry.waitForProcess)
        assertEquals("/images/game.png", entry.coverPath)
        assertEquals(emptyList(), entry.arguments)
    }

    @Test
    fun `invalid required fields and unmatched quotes reject the draft`() {
        val form = emulatorForm()
        form.name = " "
        assertNull(form.buildEntry(null))
        form.name = "Emulator"
        form.arguments = "--profile \"unfinished"
        assertNull(form.buildEntry(null))
        form.arguments = "{ROM}"
        form.extensions = ", ,"
        assertNull(form.buildEntry(null))
        assertEquals("Please fill in all required fields.", form.errorMessage)
    }

    @Test
    fun `switching entry types adjusts only the default ROM arguments`() {
        val form = emulatorForm()
        form.entryType = EntryType.NATIVE
        form.applyTypeDefaults()
        assertEquals("", form.arguments)
        form.entryType = EntryType.EMULATOR
        form.applyTypeDefaults()
        assertEquals("{ROM}", form.arguments)
        form.arguments = "--profile custom {ROM}"
        form.entryType = EntryType.NATIVE
        form.applyTypeDefaults()
        assertEquals("--profile custom {ROM}", form.arguments)
    }

    @Test
    fun `editing an emulator preserves launch identifiers and per ROM overrides`() {
        val initial =
            EmulatorSystem(
                name = "Emulator",
                executablePath = "/games/emulator",
                romsDirectory = "/roms",
                extensions = listOf("rom"),
                group = "Group",
                cloudFolder = "Cloud Folder",
                driveFileId = "drive-id",
                steamAppId = 42,
                steamProcessName = "game-process",
                savePathsByRom = mapOf("game.rom" to listOf("/saves/game.dat")),
                saveTargetsByRom =
                    mapOf(
                        "game.rom" to
                            listOf(SaveTarget("/saves/game.dat", SaveTargetType.FILE, "slot.dat"))
                    ),
                coverPathByRom = mapOf("game.rom" to "/covers/game.png"),
                titleByRom = mapOf("game.rom" to "Custom Title"),
                coversDirectory = "/covers",
            )
        val form = EntryFormState(initial).apply { name = "Renamed" }

        val entry = assertIs<EmulatorSystem>(form.buildEntry(initial))

        assertEquals(initial.copy(name = "Renamed"), entry)
    }

    @Test
    fun `new directory targets get cloud prefixes while existing legacy targets retain their layout`(
        @TempDir directory: File
    ) {
        val targets =
            listOf("one", "two").map {
                SaveTarget(File(directory, it).path, SaveTargetType.DIRECTORY)
            }
        val fresh = emulatorForm()
        fresh.saves.updatePath(0, targets[0].path)
        fresh.saves.updateType(0, SaveTargetType.DIRECTORY)
        fresh.saves.addPath()
        fresh.saves.updatePath(1, targets[1].path)
        fresh.saves.updateType(1, SaveTargetType.DIRECTORY)
        assertEquals(
            listOf("save-target-1", "save-target-2"),
            assertNotNull(fresh.buildEntry(null)).saveTargets.map { it.cloudPath },
        )

        val initial =
            EmulatorSystem(
                "Existing",
                "/emulator",
                romsDirectory = "/roms",
                extensions = listOf("rom"),
                saveTargets = targets,
            )
        assertEquals(
            targets,
            assertNotNull(EntryFormState(initial).buildEntry(initial)).saveTargets,
        )
    }

    @Test
    fun `unsafe cloud mappings are rejected before required field validation`(
        @TempDir directory: File
    ) {
        val form = EntryFormState(null)
        form.saves.updatePath(0, File(directory, "save.dat").path)
        form.saves.updateCloudPath(0, "../other-game.dat")

        assertNull(form.buildEntry(null))
        assertEquals("Cloud paths must be relative and cannot contain '..'.", form.errorMessage)
    }

    private fun emulatorForm() =
        EntryFormState(null).apply {
            name = "Emulator"
            executablePath = "/emulator"
            romsDirectory = "/roms"
            extensions = "rom"
        }
}
