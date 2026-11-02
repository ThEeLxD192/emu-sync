package com.emusync.drive

import com.emusync.domain.config.AppConfig
import com.emusync.domain.config.OAuthCredentials
import com.emusync.domain.execution.BrowserLauncher
import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.http.HttpHeaders
import io.ktor.http.headersOf
import io.ktor.serialization.kotlinx.json.json
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test

class OAuthCredentialStoreTest {
    @Test
    fun `refresh rejects a changed store revision even when credential values are identical`() =
        runTest {
            val config = AppConfig(OAuthCredentials("client", "secret", "refresh"))
            val store = MemoryCredentialStore(config)
            var requests = 0
            val client =
                HttpClient(
                    MockEngine {
                        requests++
                        store.replace(config)
                        respond(
                            """{"access_token":"stale-access","expires_in":3600,"token_type":"Bearer"}""",
                            headers = headersOf(HttpHeaders.ContentType, "application/json"),
                        )
                    }
                ) {
                    install(ContentNegotiation) { json() }
                }
            try {
                val flow =
                    OAuthFlow(
                        client,
                        store,
                        BrowserLauncher { error("Unexpected browser request") },
                    )

                assertFailsWith<DriveApiException> { flow.authorize(config) }

                assertEquals(1, requests)
                assertEquals(config, store.config)
            } finally {
                client.close()
            }
        }
}
