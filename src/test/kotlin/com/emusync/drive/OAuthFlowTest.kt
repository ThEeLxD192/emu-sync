package com.emusync.drive

import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import org.junit.jupiter.api.Test
import java.net.URI
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
}
