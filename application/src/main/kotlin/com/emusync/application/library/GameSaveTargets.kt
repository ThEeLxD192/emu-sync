package com.emusync.application.library

import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetAccess

fun SaveTargetAccess.gameSpecificTargets(entry: GameEntry, romName: String?): List<SaveTarget> =
    if (entry is EmulatorSystem) romName?.let { forRom(entry, it) }.orEmpty() else forEntry(entry)

fun SaveTargetAccess.gameTargets(entry: GameEntry, romName: String?): List<SaveTarget> =
    gameSpecificTargets(entry, romName).ifEmpty { forEntry(entry) }
