package com.emusync.drive

import io.ktor.client.plugins.HttpTimeoutConfig
import io.ktor.client.plugins.timeout

internal const val TRANSFER_SOCKET_TIMEOUT_MS = 60_000L
internal const val CONNECT_TIMEOUT_MS = 3_000L
internal const val METADATA_TIMEOUT_MS = 15_000L

internal fun io.ktor.client.request.HttpRequestBuilder.transferTimeout() {
    timeout {
        requestTimeoutMillis = HttpTimeoutConfig.INFINITE_TIMEOUT_MS
        socketTimeoutMillis = TRANSFER_SOCKET_TIMEOUT_MS
    }
}
