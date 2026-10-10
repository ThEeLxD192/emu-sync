package com.emusync.drive

import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.http.HttpHeaders
import io.ktor.http.headersOf
import io.ktor.serialization.kotlinx.json.json
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Test
import kotlin.test.assertEquals

class DriveSearchTest {

    @Test
    fun `listFilesInFolder follows next page tokens and combines results`() = runTest {
        val pageTokens = mutableListOf<String?>()
        val client = HttpClient(MockEngine) {
            install(ContentNegotiation) {
                json(Json { ignoreUnknownKeys = true })
            }
            engine {
                addHandler { request ->
                    pageTokens += request.url.parameters["pageToken"]
                    val response = when (request.url.parameters["pageToken"]) {
                        null -> """{"nextPageToken":"page-2","files":[{"id":"one","name":"slot-1.sav","modifiedTime":"2026-03-18T12:00:00Z"}]}"""
                        "page-2" -> """{"files":[{"id":"two","name":"subdir/slot-2.sav","modifiedTime":"2026-03-18T11:00:00Z"}]}"""
                        else -> error("Unexpected page token")
                    }
                    respond(
                        content = response,
                        headers = headersOf(HttpHeaders.ContentType, "application/json"),
                    )
                }
            }
        }

        val files = DriveSearch(client).listFilesInFolder("access-token", "folder-id")

        assertEquals(listOf(null, "page-2"), pageTokens)
        assertEquals(listOf("one", "two"), files.map { it.id })
        assertEquals(listOf("slot-1.sav", "subdir/slot-2.sav"), files.map { it.name })
    }
}
