package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.domain.config.AppConfig
import com.emusync.domain.config.OAuthCredentials
import com.emusync.domain.execution.BrowserLauncher
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.operation.recoverOperationFailure
import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.MockRequestHandleScope
import io.ktor.client.engine.mock.respond
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.client.request.HttpRequestData
import io.ktor.client.request.HttpResponseData
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
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class OAuthPersistenceTest {
    private val headers = headersOf(HttpHeaders.ContentType, "application/json")
    private val original =
        AppConfig(
            OAuthCredentials("client", "secret"),
            listOf(NativePCGame("Original", "echo")),
        )

    @Test
    fun `revoked token can reauthorize while preserving edits made during the new login`(
        @TempDir directory: File
    ) = runTest {
        val config =
            original.copy(
                cloudCredentials = original.cloudCredentials!!.copy(refreshToken = "expired")
            )
        val manager = manager(directory, config)
        var requests = 0
        val client = client {
            requests++
            if (requests == 1)
                respond("""{"error":"invalid_grant"}""", HttpStatusCode.BadRequest, headers)
            else respond(tokenResponse, headers = headers)
        }
        try {
            val flow =
                OAuthFlow(
                    client,
                    credentialStore = manager,
                    browser = BrowserLauncher { error("Browser should not be opened") },
                ) {
                    assertNull(manager.load().cloudCredentials?.refreshToken)
                    manager.update { current ->
                        current.copy(
                            entries =
                                current.entries +
                                    NativePCGame("Added during reauthentication", "echo")
                        )
                    }
                    "new-code"
                }

            assertEquals("access", flow.authorize(config, allowInteractive = true))
            assertEquals(2, requests)
            assertEquals("new-refresh", manager.load().cloudCredentials?.refreshToken)
            assertEquals(2, manager.load().entries.size)
        } finally {
            client.close()
        }
    }

    @Test
    fun `interactive authorization merges token into current configuration`(
        @TempDir directory: File
    ) = runTest {
        val manager = manager(directory)
        val waiting = CompletableDeferred<Unit>()
        val code = CompletableDeferred<String>()
        val client = client { respond(tokenResponse, headers = headers) }
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
            val authorization = async { flow.authorize(original, allowInteractive = true) }
            waiting.await()
            val added = NativePCGame("Added while waiting", "echo")
            manager.update { it.copy(entries = it.entries + added) }
            code.complete("authorization-code")

            assertEquals("access", authorization.await())
            assertEquals(listOf("Original", added.name), manager.load().entries.map { it.name })
            assertEquals("new-refresh", manager.load().cloudCredentials?.refreshToken)
        } finally {
            client.close()
        }
    }

    @Test
    fun `invalid grant clears only the rejected token and preserves current entries`(
        @TempDir directory: File
    ) = runTest {
        val config =
            original.copy(
                cloudCredentials = original.cloudCredentials!!.copy(refreshToken = "expired")
            )
        val manager = manager(directory, config)
        val client = client {
            manager.update { current ->
                current.copy(entries = current.entries + NativePCGame("New entry", "echo"))
            }
            respond("""{"error":"invalid_grant"}""", HttpStatusCode.BadRequest, headers)
        }
        try {
            assertFailsWith<DriveApiException> {
                OAuthFlow(
                        client,
                        credentialStore = manager,
                        browser = BrowserLauncher { error("Browser should not be opened") },
                    )
                    .authorize(config)
            }

            assertNull(manager.load().cloudCredentials?.refreshToken)
            assertEquals(listOf("Original", "New entry"), manager.load().entries.map { it.name })
        } finally {
            client.close()
        }
    }

    @Test
    fun `late invalid grant cannot clear a replacement refresh token`(@TempDir directory: File) =
        runTest {
            val config =
                original.copy(
                    cloudCredentials = original.cloudCredentials!!.copy(refreshToken = "expired")
                )
            val manager = manager(directory, config)
            val client = client {
                manager.updateCredentials { credentials ->
                    checkNotNull(credentials).copy(refreshToken = "replacement")
                }
                respond("""{"error":"invalid_grant"}""", HttpStatusCode.BadRequest, headers)
            }
            try {
                assertFailsWith<DriveApiException> {
                    OAuthFlow(
                            client,
                            credentialStore = manager,
                            browser = BrowserLauncher { error("Browser should not be opened") },
                        )
                        .authorize(config)
                }

                assertEquals("replacement", manager.load().cloudCredentials?.refreshToken)
            } finally {
                client.close()
            }
        }

    @Test
    fun `unlink cancels pending first login even when token was already null`(
        @TempDir directory: File
    ) = runTest {
        val manager = manager(directory)
        val waiting = CompletableDeferred<Unit>()
        val code = CompletableDeferred<String>()
        val client = client { error("A cancelled connection must not exchange its code") }
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
            val authorization = async {
                recoverOperationFailure(
                    action = {
                        Result.success(flow.authorize(original, allowInteractive = true))
                    },
                    recover = { Result.failure(it) },
                )
            }
            waiting.await()
            manager.updateCredentials { credentials ->
                checkNotNull(credentials).copy(refreshToken = null)
            }
            code.complete("late-code")

            assertTrue(authorization.await().exceptionOrNull() is DriveApiException)
            assertNull(manager.load().cloudCredentials?.refreshToken)
        } finally {
            client.close()
        }
    }

    @Test
    fun `unlink during code exchange rejects the returned token`(@TempDir directory: File) =
        runTest {
            val manager = manager(directory)
            val client = client {
                manager.updateCredentials { credentials ->
                    checkNotNull(credentials).copy(refreshToken = null)
                }
                respond(tokenResponse, headers = headers)
            }
            try {
                assertFailsWith<DriveApiException> {
                    OAuthFlow(
                            client,
                            credentialStore = manager,
                            browser = BrowserLauncher { error("Browser should not be opened") },
                        ) {
                            "code"
                        }
                        .authorize(original, allowInteractive = true)
                }
                assertNull(manager.load().cloudCredentials?.refreshToken)
            } finally {
                client.close()
            }
        }

    @Test
    fun `valid refresh response is rejected when connection changed while waiting`(
        @TempDir directory: File
    ) = runTest {
        val config =
            original.copy(
                cloudCredentials =
                    original.cloudCredentials!!.copy(refreshToken = "original-refresh")
            )
        val manager = manager(directory, config)
        val client = client {
            manager.updateCredentials {
                OAuthCredentials("new-client", "new-secret", "new-refresh")
            }
            respond(
                """{"access_token":"old-access","expires_in":3600,"token_type":"Bearer"}""",
                headers = headers,
            )
        }
        try {
            assertFailsWith<DriveApiException> {
                OAuthFlow(
                        client,
                        credentialStore = manager,
                        browser = BrowserLauncher { error("Browser should not be opened") },
                    )
                    .authorize(config)
            }
            assertEquals("new-client", manager.load().cloudCredentials?.clientId)
            assertEquals("new-refresh", manager.load().cloudCredentials?.refreshToken)
        } finally {
            client.close()
        }
    }

    @Test
    fun `authorization with stale credentials makes no token request`(@TempDir directory: File) =
        runTest {
            val manager = manager(directory)
            manager.updateCredentials { OAuthCredentials("new-client", "new-secret") }
            val client = client { error("Stale credentials must not be sent") }
            try {
                assertFailsWith<DriveApiException> {
                    OAuthFlow(
                            client,
                            credentialStore = manager,
                            browser = BrowserLauncher { error("Browser should not be opened") },
                        )
                        .authorize(original, allowInteractive = true)
                }
            } finally {
                client.close()
            }
        }

    @Test
    fun `provider error text alone does not invalidate refresh token`(@TempDir directory: File) =
        runTest {
            val config =
                original.copy(
                    cloudCredentials =
                        original.cloudCredentials!!.copy(refreshToken = "valid-refresh")
                )
            val manager = manager(directory, config)
            val client = client {
                respond(
                    """{"error":"temporarily_unavailable","error_description":"Retry later; this is not invalid_grant"}""",
                    HttpStatusCode.ServiceUnavailable,
                    headers,
                )
            }
            try {
                assertFailsWith<DriveApiException> {
                    OAuthFlow(
                            client,
                            credentialStore = manager,
                            browser = BrowserLauncher { error("Browser should not be opened") },
                        )
                        .authorize(config)
                }
                assertEquals("valid-refresh", manager.load().cloudCredentials?.refreshToken)
            } finally {
                client.close()
            }
        }

    @Test
    fun `cancelled refresh propagates cancellation without changing credentials`(
        @TempDir directory: File
    ) = runTest {
        val config =
            original.copy(
                cloudCredentials = original.cloudCredentials!!.copy(refreshToken = "valid-refresh")
            )
        val manager = manager(directory, config)
        val client = client { throw CancellationException("Cancelled token request") }
        try {
            assertFailsWith<CancellationException> {
                OAuthFlow(
                        client,
                        credentialStore = manager,
                        browser = BrowserLauncher { error("Browser should not be opened") },
                    )
                    .authorize(config)
            }
            assertEquals(config, manager.load())
        } finally {
            client.close()
        }
    }

    private val tokenResponse =
        """{"access_token":"access","refresh_token":"new-refresh","expires_in":3600,"token_type":"Bearer"}"""

    private fun manager(directory: File, config: AppConfig = original) =
        ConfigManager(File(directory, "config.json")).also { it.save(config) }

    private fun client(
        handler: suspend MockRequestHandleScope.(HttpRequestData) -> HttpResponseData
    ) =
        HttpClient(MockEngine(handler)) {
            install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
        }
}
