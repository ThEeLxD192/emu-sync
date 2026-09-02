package com.emusync.drive

import io.ktor.client.*
import io.ktor.client.engine.cio.*
import io.ktor.client.plugins.*
import io.ktor.client.plugins.contentnegotiation.*
import io.ktor.serialization.kotlinx.json.*
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
            requestTimeoutMillis = 10_000  // 10 seconds max for request execution
            socketTimeoutMillis = 10_000   // 10 seconds socket inactivity limit
        }
    }
}
