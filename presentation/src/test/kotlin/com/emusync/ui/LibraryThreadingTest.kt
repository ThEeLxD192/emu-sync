package com.emusync.ui

import com.emusync.application.connection.CloudConnection
import com.emusync.domain.config.AppConfig
import com.emusync.domain.config.CloudAuthorization
import com.emusync.domain.config.ConfigurationStore
import com.emusync.domain.config.OAuthCredentials
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.SteamLibrary
import com.emusync.domain.sync.SaveSynchronizerFactory
import com.emusync.local.saves.LocalSaveTargets
import com.emusync.ui.library.GameItem
import com.emusync.ui.library.LibraryServices
import com.emusync.ui.updates.UpdateServices
import com.emusync.update.DesktopUpdateInstaller
import com.emusync.update.GitHubUpdateSource
import com.emusync.update.UpdateStagingFiles
import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respondOk
import java.util.concurrent.Executors
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test

class LibraryThreadingTest {
    @Test
    fun `library IO runs on its worker while Compose reads cached Steam state`() = runTest {
        val first = NativePCGame("First", "first.exe")
        val second = NativePCGame("Second", "second.exe")
        var current = AppConfig(entries = listOf(first, second))
        var reads = 0
        fun worker() {
            assertEquals("library-worker", Thread.currentThread().name)
            reads++
        }
        val store =
            object : ConfigurationStore {
                override fun loadConfig(): AppConfig {
                    worker()
                    return current
                }

                override fun updateConfig(transform: (AppConfig) -> AppConfig): AppConfig {
                    worker()
                    current = transform(current)
                    return current
                }

                override fun updateCredentials(
                    transform: (OAuthCredentials?) -> OAuthCredentials?
                ) = updateConfig { it.copy(cloudCredentials = transform(it.cloudCredentials)) }
            }
        val steam =
            object : SteamLibrary {
                override fun isAvailable(): Boolean {
                    worker()
                    return true
                }

                override fun isRegistered(entry: GameEntry): Boolean {
                    worker()
                    return entry.steamAppId != null
                }

                override fun migrateLaunchers(entries: List<GameEntry>): Map<String, Int> {
                    worker()
                    return emptyMap()
                }

                override fun register(entry: GameEntry): Int {
                    worker()
                    return 42
                }

                override fun unregister(entry: GameEntry) {
                    worker()
                }
            }
        HttpClient(MockEngine { respondOk() }).use { client ->
            Executors.newSingleThreadExecutor { Thread(it, "library-worker") }
                .asCoroutineDispatcher()
                .use { io ->
                    val model =
                        AppViewModel(
                            LibraryServices(
                                store,
                                steam,
                                SaveSynchronizerFactory { _, _ ->
                                    error("Unexpected cloud access")
                                },
                                {},
                                { entry ->
                                    worker()
                                    listOf(GameItem(entry.name, entry))
                                },
                                LocalSaveTargets(),
                            ),
                            CloudConnection(
                                store,
                                CloudAuthorization { _, _ -> error("Unexpected authorization") },
                            ),
                            UpdateServices(
                                GitHubUpdateSource(client, "owner", "repo", "1.0.0", false),
                                UpdateStagingFiles({ false }, { null }),
                                DesktopUpdateInstaller(),
                            ),
                            io,
                        )
                    model.loadConfig()
                    assertTrue(model.isSteamAvailable())
                    assertFalse(model.isSteamRegistered(first))
                    model.reorderEntriesList(listOf(second, first))
                    assertEquals(listOf("Second", "First"), current.entries.map { it.name })
                    model.registerInSteam(first)
                    assertTrue(model.isSteamRegistered(first))
                    val registered = current.entries.first { it.name == first.name }
                    model.editGameOverride(
                        GameItem(first.name, registered),
                        emptyList(),
                        newTitle = "Renamed",
                    )
                    val renamed = current.entries.first { it.name == "Renamed" }
                    assertTrue(model.isSteamRegistered(renamed))
                    assertFalse(model.isSteamRegistered(first))
                    val before = reads
                    repeat(20) {
                        assertTrue(model.isSteamAvailable())
                        assertTrue(model.isSteamRegistered(renamed))
                    }
                    assertEquals(before, reads)
                    model.unregisterFromSteam(renamed)
                    assertFalse(model.isSteamRegistered(renamed))
                    model.unlinkGoogleDrive()
                }
        }
    }
}
