package com.emusync.drive

import io.ktor.client.HttpClient
import io.ktor.client.engine.cio.CIO
import io.ktor.client.plugins.HttpTimeout
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.serialization.kotlinx.json.json
import kotlinx.serialization.json.Json

/**
 * Shared Ktor HttpClient configured for Google Drive API interactions.
 *
 * Uses CIO engine (coroutine-based, non-blocking) and kotlinx.serialization for JSON parsing.
 * Configured with strict timeouts to prevent hanging when offline.
 */
object DriveClientFactory {

    /** Creates a configured [HttpClient] for Google API calls with timeout protection. */
    fun create(): HttpClient =
        HttpClient(CIO) {
            install(ContentNegotiation) {
                json(
                    Json {
                        ignoreUnknownKeys = true
                        prettyPrint = false
                    }
                )
            }
            install(HttpTimeout) {
                connectTimeoutMillis = CONNECT_TIMEOUT_MS // 3 seconds to establish connection
                requestTimeoutMillis =
                    METADATA_TIMEOUT_MS // Metadata/API requests should fail promptly
                socketTimeoutMillis =
                    TRANSFER_SOCKET_TIMEOUT_MS // Transfers fail only after sustained inactivity
            }
        }
}
