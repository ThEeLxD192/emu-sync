package com.emusync.domain.execution

import com.emusync.domain.library.EmulatorSystem

fun interface SteamLaunchPreparation {
    fun prepare(entry: EmulatorSystem, romPath: String)
}
