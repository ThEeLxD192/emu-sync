package com.emusync.ui.library

import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetType
import com.emusync.local.saves.LocalSaveTargets
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class GameItemTest {
    @Test
    fun `equivalent displayed games retain their identity across resolver instances and copies`(
        @TempDir directory: File
    ) {
        val entry = NativePCGame("Game", "game.exe")
        val item = GameItem("Game", entry, saveTargets = LocalSaveTargets())
        val sameGame = GameItem("Game", entry, saveTargets = LocalSaveTargets())
        val selection = setOf(item)
        assertTrue(sameGame in selection)
        assertFalse(item.copy(romFile = File(directory, "another.rom")) in selection)

        val targets = listOf(SaveTarget(File(directory, "new-save.dat").path, SaveTargetType.FILE))
        val edited = item.copy(entry = entry.copy(saveTargets = targets))
        assertEquals(targets, edited.effectiveSaveTargets)
        assertFalse(edited in selection)
    }
}
