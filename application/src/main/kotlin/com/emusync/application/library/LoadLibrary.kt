package com.emusync.application.library

import com.emusync.domain.config.AppConfig
import com.emusync.domain.config.ConfigurationStore
import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.SteamLibrary

class LoadLibrary(private val store: ConfigurationStore, private val steam: SteamLibrary) {
    fun load(): AppConfig {
        val config = store.loadConfig()
        val shortcuts = steam.migrateLaunchers(config.entries)
        val entries =
            config.entries.map { entry ->
                entry.withSteamAppId(shortcuts["[EmuSync] ${entry.name}"])
            }
        return if (entries != config.entries) store.updateConfig { it.copy(entries = entries) }
        else config
    }
}

internal fun GameEntry.withSteamAppId(appId: Int?): GameEntry =
    when (this) {
        is EmulatorSystem -> copy(steamAppId = appId)
        is NativePCGame -> copy(steamAppId = appId)
    }
