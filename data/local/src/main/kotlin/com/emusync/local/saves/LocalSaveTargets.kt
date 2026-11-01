package com.emusync.local.saves

import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.SaveTargetAccess
import com.emusync.domain.library.SaveTargetType

class LocalSaveTargets : SaveTargetAccess {
    override fun typeOf(path: String) =
        path
            .takeIf { it.isNotBlank() }
            ?.let {
                inferLegacySaveTarget(it).type.takeUnless { type -> type == SaveTargetType.UNKNOWN }
            }

    override fun forEntry(entry: GameEntry) = entry.effectiveSaveTargets()

    override fun forRom(entry: EmulatorSystem, romName: String) =
        entry.effectiveSaveTargetsForRom(romName)

    override fun forRestore(destination: String, cloudNames: List<String>) =
        resolveCloudRestoreTargets(destination, cloudNames)
}
