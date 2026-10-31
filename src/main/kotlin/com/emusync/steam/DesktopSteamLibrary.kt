package com.emusync.steam

import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.SteamLibrary

class DesktopSteamLibrary(private val manager: SteamShortcutManager) : SteamLibrary {
    override fun migrateLaunchers(entries: List<GameEntry>) =
        manager
            .migrateLegacyLaunchers(entries)
            .groupBy { it.appName }
            .mapValues { (_, shortcuts) -> shortcuts.first().appId }

    override fun register(entry: GameEntry) = manager.registerEntry(entry).appId

    override fun unregister(entry: GameEntry) = manager.unregisterEntry(entry)
}
