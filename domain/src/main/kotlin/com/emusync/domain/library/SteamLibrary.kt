package com.emusync.domain.library

/** Desktop shortcuts without exposing VDF or launcher artifacts. */
interface SteamLibrary {
    fun isAvailable(): Boolean

    fun isRegistered(entry: GameEntry): Boolean

    fun migrateLaunchers(entries: List<GameEntry>): Map<String, Int>

    fun register(entry: GameEntry): Int

    fun unregister(entry: GameEntry)
}
