package com.emusync.ui

import com.emusync.config.ConfigManager
import com.emusync.domain.config.AppConfig
import com.emusync.domain.config.OAuthCredentials
import com.emusync.domain.execution.BrowserLauncher
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetType
import com.emusync.drive.OAuthFlow
import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import io.ktor.serialization.kotlinx.json.json
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class DriveConnectionTest {
    private val game = NativePCGame("Original", "echo")
    private val headers = headersOf(HttpHeaders.ContentType, "application/json")

    @Test
    fun `browser timeout returns an actionable login error and preserves configuration`(
        @TempDir directory: File
    ) = runTest {
        val manager = manager(directory)
        val original = manager.load()
        val client = client()
        try {
            val flow =
                OAuthFlow(
                    client,
                    credentialStore = manager,
                    browser = BrowserLauncher { error("Browser should not be opened") },
                ) {
                    awaitCancellation()
                }
            val viewModel = AppViewModel(manager, httpClient = client, oauthFlow = flow)
            viewModel.loadConfig()

            val result = viewModel.loginGoogleDrive()

            assertTrue(result.contains("timed out"))
            assertEquals(original, manager.load())
            assertNull(viewModel.uiState.value.config?.cloudCredentials?.refreshToken)
        } finally {
            client.close()
        }
    }

    @Test
    fun `adding an entry while browser authorization waits keeps the entry and new token`(
        @TempDir directory: File
    ) = runTest {
        val manager = manager(directory)
        val client = client()
        val waiting = CompletableDeferred<Unit>()
        val code = CompletableDeferred<String>()
        try {
            val flow =
                OAuthFlow(
                    client,
                    credentialStore = manager,
                    browser = BrowserLauncher { error("Browser should not be opened") },
                ) {
                    waiting.complete(Unit)
                    code.await()
                }
            val viewModel = AppViewModel(manager, httpClient = client, oauthFlow = flow)
            viewModel.loadConfig()
            val login = async { viewModel.loginGoogleDrive() }
            waiting.await()
            val added = NativePCGame("Added during login", "echo")
            viewModel.addEntry(added)
            code.complete("code")

            assertEquals("Google Drive successfully connected!", login.await())
            assertEquals(listOf(game.name, added.name), manager.load().entries.map { it.name })
            assertEquals("new-refresh", manager.load().cloudCredentials?.refreshToken)
            assertEquals(manager.load(), viewModel.uiState.value.config)
            assertEquals(CloudSyncStatus.IDLE, viewModel.uiState.value.entrySyncStatus[added.name])
        } finally {
            client.close()
        }
    }

    @Test
    fun `library edits preserve credentials updated outside the current UI snapshot`(
        @TempDir directory: File
    ) = runTest {
        val manager = manager(directory, "old-refresh")
        val client = client()
        try {
            val viewModel = AppViewModel(manager, httpClient = client)
            viewModel.loadConfig()
            manager.updateCredentials { it!!.copy(refreshToken = "new-refresh") }

            viewModel.addEntry(NativePCGame("Added", "echo"))

            assertEquals("new-refresh", manager.load().cloudCredentials?.refreshToken)
            assertEquals(
                "new-refresh",
                viewModel.uiState.value.config?.cloudCredentials?.refreshToken,
            )
            assertEquals(2, manager.load().entries.size)
        } finally {
            client.close()
        }
    }

    @Test
    fun `changing OAuth client clears a token issued for the previous client`(
        @TempDir directory: File
    ) = runTest {
        val manager = manager(directory, "old-refresh")
        val client = client()
        try {
            val viewModel = AppViewModel(manager, httpClient = client)
            viewModel.loadConfig()

            viewModel.setupGoogleDrive("new-client", "new-secret", connectNow = false)

            assertEquals("new-client", manager.load().cloudCredentials?.clientId)
            assertNull(manager.load().cloudCredentials?.refreshToken)
            assertEquals(
                CloudSyncStatus.NOT_CONFIGURED,
                viewModel.uiState.value.entrySyncStatus[game.name],
            )
            assertEquals(listOf(game), manager.load().entries)
        } finally {
            client.close()
        }
    }

    @Test
    fun `saving unchanged credentials retains the existing refresh token`(
        @TempDir directory: File
    ) = runTest {
        val manager = manager(directory, "existing-refresh")
        val client = client()
        try {
            val viewModel = AppViewModel(manager, httpClient = client)
            viewModel.loadConfig()

            viewModel.setupGoogleDrive(" client ", " secret ", connectNow = false)

            assertEquals("existing-refresh", manager.load().cloudCredentials?.refreshToken)
        } finally {
            client.close()
        }
    }

    @Test
    fun `unlink resets entry sync indicators`(@TempDir directory: File) = runTest {
        val manager = manager(directory, "existing-refresh")
        val client = client()
        try {
            val viewModel = AppViewModel(manager, httpClient = client)
            viewModel.loadConfig()
            assertEquals(CloudSyncStatus.IDLE, viewModel.uiState.value.entrySyncStatus[game.name])

            viewModel.unlinkGoogleDrive()

            assertNull(viewModel.uiState.value.config?.cloudCredentials?.refreshToken)
            assertEquals(
                CloudSyncStatus.NOT_CONFIGURED,
                viewModel.uiState.value.entrySyncStatus[game.name],
            )
        } finally {
            client.close()
        }
    }

    @Test
    fun `pending login cannot reconnect Drive after unlink`(@TempDir directory: File) = runTest {
        val manager = manager(directory)
        val client = client()
        val waiting = CompletableDeferred<Unit>()
        val code = CompletableDeferred<String>()
        try {
            val flow =
                OAuthFlow(
                    client,
                    credentialStore = manager,
                    browser = BrowserLauncher { error("Browser should not be opened") },
                ) {
                    waiting.complete(Unit)
                    code.await()
                }
            val viewModel = AppViewModel(manager, httpClient = client, oauthFlow = flow)
            viewModel.loadConfig()
            val login = async { viewModel.loginGoogleDrive() }
            waiting.await()

            viewModel.unlinkGoogleDrive()
            code.complete("late-code")

            assertTrue(login.await().startsWith("Google Drive login failed:"))
            assertNull(manager.load().cloudCredentials?.refreshToken)
            assertEquals(manager.load(), viewModel.uiState.value.config)
            assertEquals(
                CloudSyncStatus.NOT_CONFIGURED,
                viewModel.uiState.value.entrySyncStatus[game.name],
            )
        } finally {
            client.close()
        }
    }

    @Test
    fun `background invalid grant updates the UI connection status`(@TempDir directory: File) =
        runTest {
            val manager = manager(directory, "expired-refresh")
            manager.update {
                it.copy(
                    entries =
                        listOf(
                            game.copy(
                                saveTargets =
                                    listOf(
                                        SaveTarget(
                                            File(directory, "save.dat").path,
                                            SaveTargetType.FILE,
                                        )
                                    )
                            )
                        )
                )
            }
            val client =
                HttpClient(
                    MockEngine {
                        respond("""{"error":"invalid_grant"}""", HttpStatusCode.BadRequest, headers)
                    }
                ) {
                    install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
                }
            try {
                val viewModel = AppViewModel(manager, httpClient = client)

                viewModel.loadConfig()

                assertNull(manager.load().cloudCredentials?.refreshToken)
                assertNull(viewModel.uiState.value.config?.cloudCredentials?.refreshToken)
                assertEquals(
                    CloudSyncStatus.NOT_CONFIGURED,
                    viewModel.uiState.value.entrySyncStatus[game.name],
                )
            } finally {
                client.close()
            }
        }

    @Test
    fun `cancelled login propagates cancellation`(@TempDir directory: File) = runTest {
        val manager = manager(directory)
        val client = client()
        try {
            val flow =
                OAuthFlow(
                    client,
                    credentialStore = manager,
                    browser = BrowserLauncher { error("Browser should not be opened") },
                ) {
                    throw CancellationException("Cancelled browser flow")
                }
            val viewModel = AppViewModel(manager, httpClient = client, oauthFlow = flow)
            viewModel.loadConfig()

            assertFailsWith<CancellationException> { viewModel.loginGoogleDrive() }
            assertNull(manager.load().cloudCredentials?.refreshToken)
        } finally {
            client.close()
        }
    }

    private fun manager(directory: File, token: String? = null) =
        ConfigManager(File(directory, "config.json")).also {
            it.save(AppConfig(OAuthCredentials("client", "secret", token), listOf(game)))
        }

    private fun client() =
        HttpClient(
            MockEngine {
                respond(
                    """{"access_token":"new-access","refresh_token":"new-refresh","expires_in":3600,"token_type":"Bearer"}""",
                    headers = headers,
                )
            }
        ) {
            install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
        }
}
