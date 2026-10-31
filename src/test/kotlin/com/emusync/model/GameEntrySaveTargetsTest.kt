package com.emusync.model

import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetType
import java.io.File
import kotlin.test.assertEquals
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class GameEntrySaveTargetsTest {
    @Test
    fun `existing save paths resolve their actual type and normalize blank cloud paths`(
        @TempDir directory: File
    ) {
        val file = File(directory, "save.dat").apply { writeText("save") }
        val folder = File(directory, "saves").apply { mkdir() }
        for ((path, expectedType) in
            listOf(file to SaveTargetType.FILE, folder to SaveTargetType.DIRECTORY)) {
            val game =
                NativePCGame(
                    name = "Game",
                    executablePath = "game",
                    saveTargets = SaveTargetType.entries.map { SaveTarget(path.path, it, " ") },
                )

            assertEquals(
                SaveTargetType.entries.map { SaveTarget(path.path, expectedType) },
                game.effectiveSaveTargets(),
            )
        }
    }

    @Test
    fun `missing save paths preserve declared types and explicit cloud names`(
        @TempDir directory: File
    ) {
        val missing = File(directory, "missing").path
        val targets = SaveTargetType.entries.map { SaveTarget(missing, it, "profile/save.dat") }
        val game = NativePCGame(name = "Game", executablePath = "game", saveTargets = targets)

        assertEquals(targets, game.effectiveSaveTargets())
        assertEquals(
            SaveTargetType.entries.map { SaveTarget(missing, it) },
            game
                .copy(saveTargets = targets.map { it.copy(cloudPath = " ") })
                .effectiveSaveTargets(),
        )
    }

    @Test
    fun `ROM target lookup preserves typed exact stem and legacy fallback priority`(
        @TempDir directory: File
    ) {
        val typedExact = SaveTarget(File(directory, "exact").path, SaveTargetType.FILE)
        val typedStem = SaveTarget(File(directory, "stem").path, SaveTargetType.DIRECTORY)
        val legacyExact = File(directory, "legacy-exact").path
        val legacyStem = File(directory, "legacy-stem").path
        val emulator =
            EmulatorSystem(
                name = "SNES",
                executablePath = "snes9x",
                romsDirectory = directory.path,
                extensions = listOf("sfc"),
                saveTargetsByRom =
                    mapOf("Game.sfc" to listOf(typedExact), "Game" to listOf(typedStem)),
                savePathsByRom =
                    mapOf("Game.sfc" to listOf(legacyExact), "Game" to listOf(legacyStem)),
            )

        assertEquals(listOf(typedExact), emulator.effectiveSaveTargetsForRom("Game.sfc"))
        val withoutExact = emulator.copy(saveTargetsByRom = mapOf("Game" to listOf(typedStem)))
        assertEquals(listOf(typedStem), withoutExact.effectiveSaveTargetsForRom("Game.sfc"))
        val legacy = emulator.copy(saveTargetsByRom = emptyMap())
        assertEquals(
            listOf(SaveTarget(legacyExact, SaveTargetType.UNKNOWN)),
            legacy.effectiveSaveTargetsForRom("Game.sfc"),
        )
        assertEquals(
            listOf(SaveTarget(legacyStem, SaveTargetType.UNKNOWN)),
            legacy
                .copy(savePathsByRom = mapOf("Game" to listOf(legacyStem)))
                .effectiveSaveTargetsForRom("Game.sfc"),
        )
        assertEquals(emptyList(), emulator.effectiveSaveTargetsForRom("Other.sfc"))
        assertEquals(
            emptyList(),
            emulator
                .copy(saveTargetsByRom = mapOf("Game.sfc" to emptyList()))
                .effectiveSaveTargetsForRom("Game.sfc"),
        )
    }
}
