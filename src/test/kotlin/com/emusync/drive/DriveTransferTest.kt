package com.emusync.drive

import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.respondError
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.client.plugins.HttpTimeout
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.content.OutgoingContent
import io.ktor.http.headersOf
import io.ktor.serialization.kotlinx.json.json
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.assertThrows
import org.junit.jupiter.api.io.TempDir
import java.io.File
import java.io.IOException
import java.time.Instant
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import io.ktor.utils.io.ByteChannel
import io.ktor.utils.io.writeFully

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
    fun `download streams a slow large response and reports progress`(@TempDir tempDir: File) = runTest {
        val destination = File(tempDir, "large-save.bin")
        val chunk = ByteArray(1024 * 1024) { (it % 251).toByte() }
        val chunkCount = 12
        val totalSize = chunk.size.toLong() * chunkCount
        val progress = mutableListOf<TransferProgress>()
        val client = HttpClient(MockEngine) {
            install(HttpTimeout) {
                requestTimeoutMillis = 25
                socketTimeoutMillis = 1_000
            }
            engine {
                addHandler {
                    val body = ByteChannel(autoFlush = true)
                    backgroundScope.launch {
                        repeat(chunkCount) {
                            delay(10)
                            body.writeFully(chunk)
                        }
                        body.close()
                    }
                    respond(
                        content = body,
                        headers = headersOf(HttpHeaders.ContentLength, totalSize.toString()),
                    )
                }
            }
        }

        DriveTransfer(client, progress::add).download("token", "large-file", destination)

        assertEquals(totalSize, destination.length())
        assertTrue(progress.size > 1)
        assertEquals(totalSize, progress.last().transferredBytes)
        assertEquals(totalSize, progress.last().totalBytes)
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

    @Test
    fun `upload recovers from lost chunk response using server acknowledged range`(@TempDir tempDir: File) = runTest {
        val chunkSize = 8 * 1024 * 1024
        val file = File(tempDir, "interrupted.sav").apply { writeBytes(ByteArray(chunkSize + 9) { it.toByte() }) }
        val requestRanges = mutableListOf<String?>()
        var lostFirstResponse = false
        val client = HttpClient(MockEngine) {
            install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
            engine {
                addHandler { request ->
                    when (request.method.value) {
                        "POST" -> respond(content = "", headers = headersOf(HttpHeaders.Location, "https://uploads.example/recover"))
                        "PUT" -> {
                            val range = request.headers[HttpHeaders.ContentRange]
                            requestRanges += range
                            when {
                                range == "bytes */${chunkSize + 9}" -> respond(
                                    content = "",
                                    status = HttpStatusCode.fromValue(308),
                                    headers = headersOf(HttpHeaders.Range, "bytes=0-${chunkSize - 1}"),
                                )
                                !lostFirstResponse -> {
                                    lostFirstResponse = true
                                    throw IOException("Connection reset after server accepted chunk")
                                }
                                else -> respond(
                                    content = """{"id":"recovered-id","name":"interrupted.sav","modifiedTime":"2026-03-18T12:00:00Z"}""",
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

        assertEquals("recovered-id", result.id)
        assertEquals(
            listOf<String?>(
                "bytes 0-${chunkSize - 1}/${chunkSize + 9}",
                "bytes */${chunkSize + 9}",
                "bytes $chunkSize-${chunkSize + 8}/${chunkSize + 9}",
            ),
            requestRanges,
        )
    }

    @Test
    fun `upload retries a chunk when server reports no bytes received`(@TempDir tempDir: File) = runTest {
        val file = File(tempDir, "retry.sav").apply { writeText("retry this save") }
        val contentRanges = mutableListOf<String?>()
        var contentRequestCount = 0
        val client = HttpClient(MockEngine) {
            install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
            engine {
                addHandler { request ->
                    when (request.method.value) {
                        "POST" -> respond(content = "", headers = headersOf(HttpHeaders.Location, "https://uploads.example/retry"))
                        "PUT" -> {
                            val range = request.headers[HttpHeaders.ContentRange]
                            contentRanges += range
                            if (range?.startsWith("bytes */") == true) {
                                respond(content = "", status = HttpStatusCode.fromValue(308))
                            } else {
                                contentRequestCount++
                                if (contentRequestCount == 1) {
                                    respond(content = "temporary outage", status = HttpStatusCode.ServiceUnavailable)
                                } else if (contentRequestCount == 2) {
                                    respond(content = "", status = HttpStatusCode.fromValue(308))
                                } else {
                                    respond(
                                        content = """{"id":"retry-id","name":"retry.sav","modifiedTime":"2026-03-18T12:00:00Z"}""",
                                        headers = jsonHeaders,
                                    )
                                }
                            }
                        }
                        else -> error("Unexpected ${request.method.value} request")
                    }
                }
            }
        }

        val result = DriveTransfer(client).upload("token", file)

        assertEquals("retry-id", result.id)
        assertEquals(3, contentRequestCount)
        assertEquals("bytes 0-${file.length() - 1}/${file.length()}", contentRanges[0])
        assertEquals("bytes */${file.length()}", contentRanges[1])
        assertEquals("bytes 0-${file.length() - 1}/${file.length()}", contentRanges[2])
    }

    @Test
    fun `upload restarts once when resumable session expired`(@TempDir tempDir: File) = runTest {
        val file = File(tempDir, "expired.sav").apply { writeText("save") }
        var sessionCount = 0
        val client = HttpClient(MockEngine) {
            install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
            engine {
                addHandler { request ->
                    when (request.method.value) {
                        "POST" -> {
                            sessionCount++
                            respond(
                                content = "",
                                headers = headersOf(HttpHeaders.Location, "https://uploads.example/session-$sessionCount"),
                            )
                        }
                        "PUT" -> if (request.url.toString().endsWith("session-1")) {
                            respond(content = "expired", status = HttpStatusCode.NotFound)
                        } else {
                            respond(
                                content = """{"id":"new-session-id","name":"expired.sav","modifiedTime":"2026-03-18T12:00:00Z"}""",
                                headers = jsonHeaders,
                            )
                        }
                        else -> error("Unexpected ${request.method.value} request")
                    }
                }
            }
        }

        val result = DriveTransfer(client).upload("token", file)

        assertEquals(2, sessionCount)
        assertEquals("new-session-id", result.id)
    }

    @Test
    fun `upload handles empty save file`() = runTest {
        val file = File.createTempFile("emusync-empty", ".sav").apply { writeBytes(byteArrayOf()) }
        var contentRange: String? = "unexpected"
        var contentLength: String? = null
        val client = HttpClient(MockEngine) {
            install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
            engine {
                addHandler { request ->
                    when (request.method.value) {
                        "POST" -> respond(content = "", headers = headersOf(HttpHeaders.Location, "https://uploads.example/empty"))
                        "PUT" -> {
                            contentRange = request.headers[HttpHeaders.ContentRange]
                            contentLength = request.headers[HttpHeaders.ContentLength]
                            respond(
                                content = """{"id":"empty-id","name":"empty.sav","modifiedTime":"2026-03-18T12:00:00Z"}""",
                                headers = jsonHeaders,
                            )
                        }
                        else -> error("Unexpected ${request.method.value} request")
                    }
                }
            }
        }

        val result = DriveTransfer(client).upload("token", file)

        assertEquals("empty-id", result.id)
        assertEquals(null, contentRange)
        assertEquals("0", contentLength)
        file.delete()
    }

    @Test
    fun `upload propagates permanent server errors instead of reporting success`(@TempDir tempDir: File) = runTest {
        val file = File(tempDir, "rejected.sav").apply { writeText("save") }
        val client = HttpClient(MockEngine) {
            install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
            engine {
                addHandler { request ->
                    when (request.method.value) {
                        "POST" -> respond(content = "", headers = headersOf(HttpHeaders.Location, "https://uploads.example/rejected"))
                        "PUT" -> respond(content = "invalid save", status = HttpStatusCode.BadRequest)
                        else -> error("Unexpected ${request.method.value} request")
                    }
                }
            }
        }

        val error = org.junit.jupiter.api.assertThrows<DriveApiException> {
            DriveTransfer(client).upload("token", file)
        }

        assertEquals(true, error.message?.contains("400"))
    }
}
