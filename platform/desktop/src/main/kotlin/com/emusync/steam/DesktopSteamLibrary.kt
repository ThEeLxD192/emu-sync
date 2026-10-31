package com.emusync.steam

import com.emusync.domain.execution.SteamLaunchPreparation
import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.SteamLibrary
import java.io.File

class DesktopSteamLibrary(private val manager: SteamShortcutManager) :
    SteamLibrary, SteamLaunchPreparation {
    override fun isAvailable() = manager.findShortcutsFile() != null

    override fun isRegistered(entry: GameEntry) = manager.isRegistered(entry)

    override fun prepare(entry: EmulatorSystem, romPath: String) =
        manager.prepareLaunch(entry, File(romPath))

    override fun migrateLaunchers(entries: List<GameEntry>) =
        manager
            .migrateLegacyLaunchers(entries)
            .groupBy { it.appName }
            .mapValues { (_, shortcuts) -> shortcuts.first().appId }

    override fun register(entry: GameEntry) = manager.registerEntry(entry).appId

    override fun unregister(entry: GameEntry) = manager.unregisterEntry(entry)
}
