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
 * Uses CIO engine (coroutine-based, non-blocking) and kotlinx.serialization
 * for JSON parsing. Configured with strict timeouts to prevent hanging when offline.
 */
object DriveClientFactory {

    /**
     * Creates a configured [HttpClient] for Google API calls with timeout protection.
     */
    fun create(): HttpClient = HttpClient(CIO) {
        install(ContentNegotiation) {
            json(Json {
                ignoreUnknownKeys = true
                prettyPrint = false
            })
        }
        install(HttpTimeout) {
            connectTimeoutMillis = 3_000   // 3 seconds to establish connection
            requestTimeoutMillis = 15_000  // Metadata/API requests should fail promptly
            socketTimeoutMillis = 60_000   // Transfers fail only after sustained inactivity
        }
    }
}
