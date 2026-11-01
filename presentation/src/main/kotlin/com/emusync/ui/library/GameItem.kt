package com.emusync.ui.library

import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import java.io.File

/**
 * Represents a displayable game item in the grid. Unifies ROMs (from scanning) and native games
 * into a single UI model.
 */
data class GameItem(
    val name: String,
    val entry: GameEntry,
    val romFile: File? = null,
) {
    val effectiveTitle: String
        get() =
            when (entry) {
                is EmulatorSystem -> {
                    val romName = romFile?.name ?: name
                    entry.getEffectiveTitle(romName, name)
                }
                is NativePCGame -> entry.name
            }

    val effectiveCoverPath: String?
        get() =
            when (entry) {
                is NativePCGame -> entry.coverPath?.takeIf { it.isNotBlank() }
                is EmulatorSystem ->
                    romFile?.name?.let { entry.coverPathByRom[it]?.takeIf { p -> p.isNotBlank() } }
            }
}
