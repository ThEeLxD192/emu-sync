package com.emusync.drive

import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.respondError
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.content.OutgoingContent
import io.ktor.http.headersOf
import io.ktor.serialization.kotlinx.json.json
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.assertThrows
import org.junit.jupiter.api.io.TempDir
import java.io.File
import java.time.Instant
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals

class DriveTransferTest {

    private val jsonHeaders = headersOf(HttpHeaders.ContentType, "application/json")

    @Test
    fun `download replaces destination and applies remote timestamp after success`(@TempDir tempDir: File) = runTest {
        val destination = File(tempDir, "saves/slot.sav").apply {
            parentFile.mkdirs()
            writeText("old save")
        }
        val bytes = byteArrayOf(1, 2, 3, 4)
        val modifiedTime = "2026-03-18T12:00:00Z"
        val client = HttpClient(MockEngine) {
            engine {
                addHandler {
                    respond(
                        content = bytes,
                        headers = headersOf(HttpHeaders.ContentType, "application/octet-stream"),
                    )
                }
            }
        }

        DriveTransfer(client).download("token", "file-id", destination, modifiedTime)

        assertContentEquals(bytes, destination.readBytes())
        assertEquals(Instant.parse(modifiedTime).toEpochMilli(), destination.lastModified())
        assertEquals(listOf("slot.sav"), destination.parentFile.list()?.toList())
    }

    @Test
    fun `failed download preserves existing save and removes temporary files`(@TempDir tempDir: File) = runTest {
        val destination = File(tempDir, "slot.sav").apply { writeText("important local progress") }
        val originalBytes = destination.readBytes()
        val originalModifiedTime = Instant.parse("2026-03-18T10:00:00Z").toEpochMilli()
        destination.setLastModified(originalModifiedTime)
        val client = HttpClient(MockEngine) {
            engine {
                addHandler { respondError(HttpStatusCode.InternalServerError) }
            }
        }

        assertThrows<DriveApiException> {
            DriveTransfer(client).download("token", "file-id", destination, "2026-03-18T12:00:00Z")
        }

        assertContentEquals(originalBytes, destination.readBytes())
        assertEquals(originalModifiedTime, destination.lastModified())
        assertEquals(listOf("slot.sav"), tempDir.list()?.toList())
    }

    @Test
    fun `upload creates resumable session and sends file content`() = runTest {
        val file = File.createTempFile("emusync-upload", ".sav").apply { writeBytes(byteArrayOf(1, 2, 3, 4)) }
        val requests = mutableListOf<io.ktor.client.request.HttpRequestData>()
        val sessionUri = "https://uploads.example/session-create"
        val client = HttpClient(MockEngine) {
            install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
            engine {
                addHandler { request ->
                    requests += request
                    when (request.method.value) {
                        "POST" -> respond(
                            content = "",
                            headers = headersOf(HttpHeaders.Location, sessionUri),
                        )
                        "PUT" -> respond(
                            content = """{"id":"created-id","name":"slot.sav","modifiedTime":"2026-03-18T12:00:00Z"}""",
                            status = HttpStatusCode.Created,
                            headers = jsonHeaders,
                        )
                        else -> error("Unexpected ${request.method.value} request")
                    }
                }
            }
        }

        val result = DriveTransfer(client).upload("token", file, "slot.sav", "save-folder")

        assertEquals("created-id", result.id)
        assertEquals("resumable", requests[0].url.parameters["uploadType"])
        assertEquals("4", requests[0].headers["X-Upload-Content-Length"])
        val metadata = Json.parseToJsonElement(
            (requests[0].body as OutgoingContent.ByteArrayContent).bytes().decodeToString()
        ).jsonObject
        assertEquals("slot.sav", metadata["name"]?.jsonPrimitive?.content)
        assertEquals("save-folder", metadata["parents"]?.jsonArray?.single()?.jsonPrimitive?.content)
        assertEquals(sessionUri, requests[1].url.toString())
        assertEquals("bytes 0-3/4", requests[1].headers[HttpHeaders.ContentRange])
        file.delete()
    }

    @Test
    fun `update starts resumable session with PATCH and uploads media`() = runTest {
        val file = File.createTempFile("emusync-update", ".sav").apply { writeText("save") }
        var sessionStarted = false
        val client = HttpClient(MockEngine) {
            install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
            engine {
                addHandler { request ->
                    when (request.method.value) {
                        "PATCH" -> {
                            sessionStarted = request.url.encodedPath.endsWith("/file-id") &&
                                request.url.parameters["uploadType"] == "resumable"
                            respond(content = "", headers = headersOf(HttpHeaders.Location, "https://uploads.example/session-update"))
                        }
                        "PUT" -> respond(
                            content = """{"id":"file-id","name":"save","modifiedTime":"2026-03-18T12:00:00Z"}""",
                            headers = jsonHeaders,
                        )
                        else -> error("Unexpected ${request.method.value} request")
                    }
                }
            }
        }

        val result = DriveTransfer(client).update("token", "file-id", file)

        assertEquals(true, sessionStarted)
        assertEquals("file-id", result.id)
        file.delete()
    }

    @Test
    fun `large upload sends aligned chunks and follows Drive range acknowledgements`(@TempDir tempDir: File) = runTest {
        val chunkSize = 8 * 1024 * 1024
        val file = File(tempDir, "large.sav").apply { writeBytes(ByteArray(chunkSize + 17) { it.toByte() }) }
        val ranges = mutableListOf<String?>()
        val client = HttpClient(MockEngine) {
            install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
            engine {
                addHandler { request ->
                    when (request.method.value) {
                        "POST" -> respond(content = "", headers = headersOf(HttpHeaders.Location, "https://uploads.example/session-large"))
                        "PUT" -> {
                            ranges += request.headers[HttpHeaders.ContentRange]
                            if (ranges.size == 1) {
                                respond(
                                    content = "",
                                    status = HttpStatusCode.fromValue(308),
                                    headers = headersOf(HttpHeaders.Range, "bytes=0-${chunkSize - 1}"),
                                )
                            } else {
                                respond(
                                    content = """{"id":"large-id","name":"large.sav","modifiedTime":"2026-03-18T12:00:00Z"}""",
                                    headers = jsonHeaders,
                                )
                            }
                        }
                        else -> error("Unexpected ${request.method.value} request")
                    }
                }
            }
        }

        val result = DriveTransfer(client).upload("token", file)

        assertEquals("large-id", result.id)
        assertEquals(
            listOf<String?>(
                "bytes 0-${chunkSize - 1}/${chunkSize + 17}",
                "bytes $chunkSize-${chunkSize + 16}/${chunkSize + 17}",
            ),
            ranges,
        )
    }
}
