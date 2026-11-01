package com.emusync.application.library

import com.emusync.domain.config.AppConfig
import com.emusync.domain.config.ConfigurationStore
import com.emusync.domain.config.OAuthCredentials
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.SteamLibrary
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import org.junit.jupiter.api.Test

class SteamRegistrationTest {
    @Test
    fun `registration uses the latest entry and retains edits made during the Steam operation`() {
        val stale = NativePCGame("Game", "old.exe")
        val latest = stale.copy(executablePath = "new.exe")
        val added = NativePCGame("Added", "added.exe")
        val store = MemoryStore(AppConfig(entries = listOf(latest)))
        val steam = RecordingSteam {
            store.current =
                store.current.copy(
                    entries = listOf(latest.copy(coverPath = "new-cover.png"), added),
                    cloudCredentials = OAuthCredentials("client", "secret", "new-refresh"),
                )
        }
        val result = SteamRegistration(store, steam).register(stale)

        assertEquals<List<GameEntry>>(listOf(latest), steam.registered)
        assertEquals(latest.copy(steamAppId = 42, coverPath = "new-cover.png"), result.entry)
        assertEquals(listOf(result.entry, added), store.current.entries)
        assertEquals("new-refresh", store.current.cloudCredentials?.refreshToken)
        assertEquals(store.current, result.config)
    }

    @Test
    fun `unregister clears only the requested Steam ID after the shortcut is removed`() {
        val game = NativePCGame("Game", "game.exe", steamAppId = 42, coverPath = "cover.png")
        val other = NativePCGame("Other", "other.exe", steamAppId = 99)
        val store = MemoryStore(AppConfig(entries = listOf(game, other)))
        val steam = RecordingSteam()
        val result = SteamRegistration(store, steam).unregister(game)

        assertEquals<List<GameEntry>>(listOf(game), steam.unregistered)
        assertEquals(listOf(game.copy(steamAppId = null), other), result.config.entries)
    }

    @Test
    fun `failed Steam registration does not persist an ID or recreate a removed entry`() {
        val game = NativePCGame("Game", "game.exe")
        val store = MemoryStore(AppConfig(entries = listOf(game)))
        val steam = RecordingSteam { error("Steam refused the shortcut") }
        assertFailsWith<IllegalStateException> { SteamRegistration(store, steam).register(game) }
        assertEquals(listOf(game), store.current.entries)
        assertEquals(0, store.writes)
        store.current = AppConfig()
        assertFailsWith<IllegalStateException> { SteamRegistration(store, steam).register(game) }
        assertEquals(1, steam.registered.size)
        assertEquals(emptyList(), store.current.entries)
    }

    private class MemoryStore(var current: AppConfig) : ConfigurationStore {
        var writes = 0

        override fun loadConfig() = current

        override fun updateConfig(transform: (AppConfig) -> AppConfig): AppConfig {
            current = transform(current)
            writes++
            return current
        }

        override fun updateCredentials(transform: (OAuthCredentials?) -> OAuthCredentials?) =
            updateConfig {
                it.copy(cloudCredentials = transform(it.cloudCredentials))
            }
    }

    private class RecordingSteam(private val duringRegister: () -> Unit = {}) : SteamLibrary {
        val registered = mutableListOf<GameEntry>()
        val unregistered = mutableListOf<GameEntry>()

        override fun isAvailable() = true

        override fun isRegistered(entry: GameEntry) = entry.steamAppId != null

        override fun migrateLaunchers(entries: List<GameEntry>) = emptyMap<String, Int>()

        override fun register(entry: GameEntry): Int {
            registered += entry
            duringRegister()
            return 42
        }

        override fun unregister(entry: GameEntry) {
            unregistered += entry
        }
    }
}
