package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.model.AppConfig
import com.emusync.model.GoogleDriveConfig
import com.emusync.model.NativePCGame
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
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNull
import kotlin.test.assertTrue

class OAuthPersistenceTest {
    private val headers = headersOf(HttpHeaders.ContentType, "application/json")
    private val original = AppConfig(
        GoogleDriveConfig("client", "secret"), listOf(NativePCGame("Original", "echo")),
    )

    @Test
    fun `revoked token can reauthorize while preserving edits made during the new login`(@TempDir directory: File) = runTest {
        val config = original.copy(googleDrive = original.googleDrive!!.copy(refreshToken = "expired"))
        val manager = manager(directory, config)
        var requests = 0
        val client = client {
            requests++
            if (requests == 1) respond("""{"error":"invalid_grant"}""", HttpStatusCode.BadRequest, headers)
            else respond(tokens(), headers = headers)
        }
        try {
            val flow = OAuthFlow(client) {
                assertNull(manager.load().googleDrive?.refreshToken)
                manager.update { it.copy(entries = it.entries + NativePCGame("Added during reauthentication", "echo")) }
                "new-code"
            }

            assertEquals("access", flow.authorize(config, manager, allowInteractive = true))
            assertEquals(2, requests)
            assertEquals("new-refresh", manager.load().googleDrive?.refreshToken)
            assertEquals(2, manager.load().entries.size)
        } finally { client.close() }
    }

    @Test
    fun `interactive authorization merges token into current configuration`(@TempDir directory: File) = runTest {
        val manager = manager(directory)
        val waiting = CompletableDeferred<Unit>()
        val code = CompletableDeferred<String>()
        val client = client { respond(tokens(), headers = headers) }
        try {
            val flow = OAuthFlow(client) { waiting.complete(Unit); code.await() }
            val authorization = async { flow.authorize(original, manager, allowInteractive = true) }
            waiting.await()
            val added = NativePCGame("Added while waiting", "echo")
            manager.update { it.copy(entries = it.entries + added) }
            code.complete("authorization-code")

            assertEquals("access", authorization.await())
            assertEquals(listOf("Original", added.name), manager.load().entries.map { it.name })
            assertEquals("new-refresh", manager.load().googleDrive?.refreshToken)
        } finally { client.close() }
    }

    @Test
    fun `invalid grant clears only the rejected token and preserves current entries`(@TempDir directory: File) = runTest {
        val config = original.copy(googleDrive = original.googleDrive!!.copy(refreshToken = "expired"))
        val manager = manager(directory, config)
        val client = client {
            manager.update { it.copy(entries = it.entries + NativePCGame("New entry", "echo")) }
            respond("""{"error":"invalid_grant"}""", HttpStatusCode.BadRequest, headers)
        }
        try {
            assertFailsWith<DriveApiException> { OAuthFlow(client).authorize(config, manager) }

            assertNull(manager.load().googleDrive?.refreshToken)
            assertEquals(listOf("Original", "New entry"), manager.load().entries.map { it.name })
        } finally { client.close() }
    }

    @Test
    fun `late invalid grant cannot clear a replacement refresh token`(@TempDir directory: File) = runTest {
        val config = original.copy(googleDrive = original.googleDrive!!.copy(refreshToken = "expired"))
        val manager = manager(directory, config)
        val client = client {
            manager.updateGoogleDrive { it!!.copy(refreshToken = "replacement") }
            respond("""{"error":"invalid_grant"}""", HttpStatusCode.BadRequest, headers)
        }
        try {
            assertFailsWith<DriveApiException> { OAuthFlow(client).authorize(config, manager) }

            assertEquals("replacement", manager.load().googleDrive?.refreshToken)
        } finally { client.close() }
    }

    @Test
    fun `unlink cancels pending first login even when token was already null`(@TempDir directory: File) = runTest {
        val manager = manager(directory)
        val waiting = CompletableDeferred<Unit>()
        val code = CompletableDeferred<String>()
        val client = client { error("A cancelled connection must not exchange its code") }
        try {
            val flow = OAuthFlow(client) { waiting.complete(Unit); code.await() }
            val authorization = async { runCatching { flow.authorize(original, manager, allowInteractive = true) } }
            waiting.await()
            manager.updateGoogleDrive { it!!.copy(refreshToken = null) }
            code.complete("late-code")

            assertTrue(authorization.await().exceptionOrNull() is DriveApiException)
            assertNull(manager.load().googleDrive?.refreshToken)
        } finally { client.close() }
    }

    @Test
    fun `unlink during code exchange rejects the returned token`(@TempDir directory: File) = runTest {
        val manager = manager(directory)
        val client = client {
            manager.updateGoogleDrive { it!!.copy(refreshToken = null) }
            respond(tokens(), headers = headers)
        }
        try {
            assertFailsWith<DriveApiException> {
                OAuthFlow(client) { "code" }.authorize(original, manager, allowInteractive = true)
            }
            assertNull(manager.load().googleDrive?.refreshToken)
        } finally { client.close() }
    }

    @Test
    fun `valid refresh response is rejected when connection changed while waiting`(@TempDir directory: File) = runTest {
        val config = original.copy(googleDrive = original.googleDrive!!.copy(refreshToken = "original-refresh"))
        val manager = manager(directory, config)
        val client = client {
            manager.updateGoogleDrive { GoogleDriveConfig("new-client", "new-secret", "new-refresh") }
            respond("""{"access_token":"old-access","expires_in":3600,"token_type":"Bearer"}""", headers = headers)
        }
        try {
            assertFailsWith<DriveApiException> { OAuthFlow(client).authorize(config, manager) }
            assertEquals("new-client", manager.load().googleDrive?.clientId)
            assertEquals("new-refresh", manager.load().googleDrive?.refreshToken)
        } finally { client.close() }
    }

    @Test
    fun `authorization with stale credentials makes no token request`(@TempDir directory: File) = runTest {
        val manager = manager(directory)
        manager.updateGoogleDrive { GoogleDriveConfig("new-client", "new-secret") }
        val client = client { error("Stale credentials must not be sent") }
        try {
            assertFailsWith<DriveApiException> { OAuthFlow(client).authorize(original, manager, allowInteractive = true) }
        } finally { client.close() }
    }

    @Test
    fun `provider error text alone does not invalidate refresh token`(@TempDir directory: File) = runTest {
        val config = original.copy(googleDrive = original.googleDrive!!.copy(refreshToken = "valid-refresh"))
        val manager = manager(directory, config)
        val client = client {
            respond("""{"error":"temporarily_unavailable","error_description":"Retry later; this is not invalid_grant"}""", HttpStatusCode.ServiceUnavailable, headers)
        }
        try {
            assertFailsWith<DriveApiException> { OAuthFlow(client).authorize(config, manager) }
            assertEquals("valid-refresh", manager.load().googleDrive?.refreshToken)
        } finally { client.close() }
    }

    @Test
    fun `cancelled refresh propagates cancellation without changing credentials`(@TempDir directory: File) = runTest {
        val config = original.copy(googleDrive = original.googleDrive!!.copy(refreshToken = "valid-refresh"))
        val manager = manager(directory, config)
        val client = client { throw CancellationException("Cancelled token request") }
        try {
            assertFailsWith<CancellationException> { OAuthFlow(client).authorize(config, manager) }
            assertEquals(config, manager.load())
        } finally { client.close() }
    }

    private fun tokens() = """{"access_token":"access","refresh_token":"new-refresh","expires_in":3600,"token_type":"Bearer"}"""
    private fun manager(directory: File, config: AppConfig = original) = ConfigManager(File(directory, "config.json")).also { it.save(config) }
    private fun client(handler: suspend MockRequestHandleScope.(HttpRequestData) -> HttpResponseData) = HttpClient(MockEngine(handler)) {
        install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
    }
}
