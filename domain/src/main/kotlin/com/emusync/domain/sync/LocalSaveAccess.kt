package com.emusync.domain.sync

import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.SaveTarget

interface LocalSaveAccess {
    fun path(path: String): LocalSavePath

    fun effectiveSaveTargets(entry: GameEntry): List<SaveTarget>

    fun effectiveSaveTargetsForRom(entry: EmulatorSystem, romName: String): List<SaveTarget>
}
