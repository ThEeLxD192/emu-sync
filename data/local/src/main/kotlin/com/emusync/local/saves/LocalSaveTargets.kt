package com.emusync.local.saves

import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.SaveTargetAccess

class LocalSaveTargets : SaveTargetAccess {
    override fun forEntry(entry: GameEntry) = entry.effectiveSaveTargets()

    override fun forRom(entry: EmulatorSystem, romName: String) =
        entry.effectiveSaveTargetsForRom(romName)

    override fun forRestore(destination: String, cloudNames: List<String>) =
        resolveCloudRestoreTargets(destination, cloudNames)
}
