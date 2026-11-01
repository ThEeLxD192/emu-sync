package com.emusync.application.library

import com.emusync.domain.config.ConfigurationStore
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.SteamLibrary

/** Keeps shortcut registration and its persisted ID together without replacing unrelated edits. */
class SteamRegistration(private val store: ConfigurationStore, private val steam: SteamLibrary) {
    fun register(entry: GameEntry): EntryEditResult = change(entry, true)

    fun unregister(entry: GameEntry): EntryEditResult = change(entry, false)

    private fun change(entry: GameEntry, register: Boolean): EntryEditResult {
        val latest =
            store.loadConfig().entries.firstOrNull { it.name == entry.name }
                ?: error("The category no longer exists.")
        val appId =
            if (register) steam.register(latest)
            else {
                steam.unregister(latest)
                null
            }
        val updated = store.updateConfig { current ->
            check(current.entries.any { it.name == latest.name }) {
                "The category no longer exists."
            }
            current.copy(
                entries =
                    current.entries.map {
                        if (it.name == latest.name) it.withSteamAppId(appId) else it
                    }
            )
        }
        return EntryEditResult(updated, updated.entries.first { it.name == latest.name })
    }
}
