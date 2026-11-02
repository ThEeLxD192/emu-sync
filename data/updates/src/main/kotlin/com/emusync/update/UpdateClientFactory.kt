package com.emusync.update

import io.ktor.client.HttpClient
import io.ktor.client.engine.cio.CIO
import io.ktor.client.plugins.HttpTimeout
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.serialization.kotlinx.json.json
import kotlinx.serialization.json.Json

private const val UPDATE_CONNECT_TIMEOUT_MS = 15_000L

fun createUpdateClient(): HttpClient =
    HttpClient(CIO) {
        install(ContentNegotiation) {
            json(
                Json {
                    ignoreUnknownKeys = true
                    isLenient = true
                }
            )
        }
        install(HttpTimeout) {
            connectTimeoutMillis = UPDATE_CONNECT_TIMEOUT_MS
            socketTimeoutMillis = UPDATE_SOCKET_TIMEOUT_MS
            requestTimeoutMillis = null
        }
    }
