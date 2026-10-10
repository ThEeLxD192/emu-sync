package com.emusync.drive

import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import com.sun.net.httpserver.HttpServer
import org.junit.jupiter.api.Test
import java.net.HttpURLConnection
import java.net.InetSocketAddress
import java.net.URI
import java.net.URL
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue

class OAuthFlowTest {
    private val flow = OAuthFlow(HttpClient(MockEngine { error("HTTP should not be called") }))

    @Test
    fun `authorization URL includes the login state`() {
        val uri = URI(flow.buildAuthUrl("client id", "random-state"))
        val params = flow.parseQueryParams(uri.rawQuery)

        assertEquals("client id", params["client_id"])
        assertEquals("random-state", params["state"])
        assertEquals("http://localhost:8089", params["redirect_uri"])
    }

    @Test
    fun `callback requires matching state before accepting code`() {
        assertEquals(
            "valid-code",
            flow.validateCallback(mapOf("code" to "valid-code", "state" to "expected"), "expected"),
        )

        assertFailsWith<DriveApiException> {
            flow.validateCallback(mapOf("code" to "injected-code"), "expected")
        }
        assertFailsWith<DriveApiException> {
            flow.validateCallback(mapOf("code" to "injected-code", "state" to "wrong"), "expected")
        }
    }

    @Test
    fun `callback reports provider errors and malformed requests`() {
        assertFailsWith<DriveApiException> {
            flow.validateCallback(mapOf("error" to "access_denied", "state" to "expected"), "expected")
        }
        assertFailsWith<DriveApiException> {
            flow.validateCallback(mapOf("state" to "expected"), "expected")
        }
        assertEquals(mapOf("state" to "ok"), flow.parseQueryParams("malformed&state=ok"))
    }

    @Test
    fun `callback server address is restricted to loopback`() {
        val address = flow.loopbackAddress()

        assertTrue(address.address.isLoopbackAddress)
        assertEquals(8089, address.port)
    }

    @Test
    fun `invalid and malformed HTTP callbacks do not prevent a valid callback`() {
        val server = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
        val results = mutableListOf<Result<String>>()
        val callbackReceived = CountDownLatch(1)
        server.createContext("/", flow.callbackHandler(server, "expected") {
            results += it
            callbackReceived.countDown()
        })
        server.start()

        try {
            val baseUrl = "http://127.0.0.1:${server.address.port}/"

            assertEquals(400, requestStatus(baseUrl))
            assertTrue(results.isEmpty())

            assertEquals(400, requestStatus("${baseUrl}?state=expected&code="))
            assertTrue(results.isEmpty())

            assertEquals(400, requestStatus(baseUrl, "state=%ZZ"))
            assertTrue(results.isEmpty())

            assertEquals(200, requestStatus("${baseUrl}?state=expected&code=valid-code"))
            assertTrue(callbackReceived.await(1, TimeUnit.SECONDS))
            assertEquals(1, results.size)
            assertEquals("valid-code", results.single().getOrThrow())
        } finally {
            runCatching { server.stop(0) }
        }
    }

    private fun requestStatus(url: String): Int {
        val connection = URI(url).toURL().openConnection() as HttpURLConnection
        return requestStatus(connection)
    }

    private fun requestStatus(baseUrl: String, rawQuery: String): Int {
        val connection = URL(URI(baseUrl).toURL(), "?$rawQuery").openConnection() as HttpURLConnection
        return requestStatus(connection)
    }

    private fun requestStatus(connection: HttpURLConnection): Int {
        return try {
            connection.requestMethod = "GET"
            connection.responseCode
        } finally {
            connection.disconnect()
        }
    }
}
