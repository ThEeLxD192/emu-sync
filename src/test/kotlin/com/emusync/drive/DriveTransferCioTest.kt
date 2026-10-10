package com.emusync.drive

import io.ktor.client.HttpClient
import io.ktor.client.engine.cio.CIO
import io.ktor.client.plugins.HttpTimeout
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.serialization.kotlinx.json.json
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.BufferedReader
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.InputStreamReader
import java.net.ServerSocket
import java.net.Socket
import java.nio.charset.StandardCharsets
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class DriveTransferCioTest {

    @Test
    fun `CIO download completes when response streams slowly beyond configured request timeout`(@TempDir tempDir: File) = runTest {
        val expected = byteArrayOf(1, 2, 3, 4)
        val server = LocalHttpServer { socket ->
            socket.readRequestHeaders()
            socket.getOutputStream().apply {
                write("HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n".toByteArray())
                write(expected, 0, 1)
                flush()
                Thread.sleep(150)
                write(expected, 1, 3)
                flush()
            }
        }
        val client = cioClientWithShortTimeouts()
        val destination = File(tempDir, "slow-save.bin")

        try {
            DriveTransfer(client, downloadFilesUrl = server.filesUrl).download("token", "slow", destination)
            server.awaitCompletion()

            assertContentEquals(expected, destination.readBytes())
        } finally {
            client.close()
            server.close()
        }
    }

    @Test
    fun `CIO stalled download remains cancellable and cleans temporary file`(@TempDir tempDir: File) = runTest {
        val responseStarted = CountDownLatch(1)
        val releaseServer = CountDownLatch(1)
        val server = LocalHttpServer { socket ->
            socket.readRequestHeaders()
            socket.getOutputStream().apply {
                write("HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n".toByteArray())
                write(byteArrayOf(7))
                flush()
            }
            responseStarted.countDown()
            releaseServer.await(3, TimeUnit.SECONDS)
        }
        val client = cioClientWithShortTimeouts()
        val destination = File(tempDir, "existing-save.bin").apply { writeText("keep me") }
        val download = launch {
            DriveTransfer(client, downloadFilesUrl = server.filesUrl).download("token", "stalled", destination)
        }

        try {
            assertTrue(
                withContext(Dispatchers.IO) { responseStarted.await(3, TimeUnit.SECONDS) },
                "The local server should begin the response",
            )
            withContext(Dispatchers.IO) { Thread.sleep(150) }
            assertFalse(download.isCompleted, "The per-transfer timeout should allow a stalled response to be cancelled by its caller")

            withTimeout(2_000) { download.cancelAndJoin() }

            assertEquals("keep me", destination.readText())
            assertEquals(listOf("existing-save.bin"), tempDir.list()?.toList())
        } finally {
            releaseServer.countDown()
            client.close()
            server.close()
        }
    }

    @Test
    fun `CIO resumable upload completes when response is slow beyond configured request timeout`(@TempDir tempDir: File) = runTest {
        val server = LocalHttpServer(connectionCount = 2) { socket ->
            val request = socket.readRequest()
            if (request.line.startsWith("POST ")) {
                socket.respond(
                    "HTTP/1.1 200 OK\r\nLocation: http://127.0.0.1:${socket.localPort}/session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
            } else {
                assertTrue(request.line.startsWith("PUT /session "))
                Thread.sleep(300)
                val responseBody = "{\"id\":\"cio-upload\",\"name\":\"save.sav\",\"modifiedTime\":\"2026-03-18T12:00:00Z\"}"
                socket.respond(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n" +
                        "Content-Length: ${responseBody.toByteArray(StandardCharsets.US_ASCII).size}\r\n" +
                        "Connection: close\r\n\r\n$responseBody",
                )
            }
        }
        val client = cioClientWithShortTimeouts()
        val file = File(tempDir, "save.sav").apply { writeBytes(byteArrayOf(1, 2, 3, 4)) }

        try {
            val result = DriveTransfer(
                client,
                server.filesUrl,
                server.uploadFilesUrl,
            ).upload("token", file)
            server.awaitCompletion()

            assertEquals("cio-upload", result.id)
        } finally {
            client.close()
            server.close()
        }
    }

    @Test
    fun `CIO caller cancellation interrupts stalled resumable upload`(@TempDir tempDir: File) = runTest {
        val uploadReceived = CountDownLatch(1)
        val releaseServer = CountDownLatch(1)
        val server = LocalHttpServer(connectionCount = 2) { socket ->
            val request = socket.readRequest()
            if (request.line.startsWith("POST ")) {
                socket.respond(
                    "HTTP/1.1 200 OK\r\nLocation: http://127.0.0.1:${socket.localPort}/session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
            } else {
                assertTrue(request.line.startsWith("PUT /session "))
                uploadReceived.countDown()
                releaseServer.await(3, TimeUnit.SECONDS)
            }
        }
        val client = cioClientWithShortTimeouts()
        val file = File(tempDir, "save.sav").apply { writeText("upload contents") }
        val upload = launch {
            DriveTransfer(client, server.filesUrl, server.uploadFilesUrl).upload("token", file)
        }

        try {
            assertTrue(
                withContext(Dispatchers.IO) { uploadReceived.await(3, TimeUnit.SECONDS) },
                "The local server should receive the resumable upload",
            )
            withContext(Dispatchers.IO) { Thread.sleep(300) }
            assertFalse(upload.isCompleted, "The stalled upload should remain active until its caller cancels it")

            withContext(Dispatchers.IO) {
                withTimeout(2_000) { upload.cancelAndJoin() }
            }
            assertTrue(upload.isCancelled)
            assertEquals("upload contents", file.readText())
        } finally {
            releaseServer.countDown()
            client.close()
            server.close()
        }
    }

    @Test
    fun `CIO resumable upload recovers when server accepts a chunk but drops its response`(@TempDir tempDir: File) = runTest {
        val contentRanges = mutableListOf<String?>()
        val server = LocalHttpServer(connectionCount = 4) { socket ->
            val request = socket.readRequest()
            when {
                request.line.startsWith("POST ") -> socket.respond(
                    "HTTP/1.1 200 OK\r\nLocation: http://127.0.0.1:${socket.localPort}/session\r\n" +
                        "Content-Length: 0\r\nConnection: close\r\n\r\n",
                )
                request.line.startsWith("PUT ") -> {
                    val range = request.headers["content-range"]
                    contentRanges += range
                    when (range) {
                        "bytes 0-3/4" -> Unit // Accept the chunk, then close the socket without a response.
                        "bytes */4" -> socket.respond(
                            "HTTP/1.1 308 Resume Incomplete\r\nRange: bytes=0-1\r\n" +
                                "Content-Length: 0\r\nConnection: close\r\n\r\n",
                        )
                        "bytes 2-3/4" -> socket.respond(uploadSuccessResponse())
                        else -> error("Unexpected upload range: $range")
                    }
                }
                else -> error("Unexpected request: ${request.line}")
            }
        }
        val client = cioClientWithShortTimeouts()
        val file = File(tempDir, "recovered.sav").apply { writeBytes(byteArrayOf(1, 2, 3, 4)) }

        try {
            val result = DriveTransfer(client, server.filesUrl, server.uploadFilesUrl).upload("token", file)
            server.awaitCompletion()

            assertEquals("cio-upload", result.id)
            assertEquals(listOf<String?>("bytes 0-3/4", "bytes */4", "bytes 2-3/4"), contentRanges)
        } finally {
            client.close()
            server.close()
        }
    }

    @Test
    fun `CIO resumable upload sends multiple chunks and follows range acknowledgements`(@TempDir tempDir: File) = runTest {
        val chunkSize = 8 * 1024 * 1024
        val totalSize = chunkSize + 17
        val contentRanges = mutableListOf<String?>()
        val server = LocalHttpServer(connectionCount = 3) { socket ->
            val request = socket.readRequest()
            when {
                request.line.startsWith("POST ") -> socket.respond(
                    "HTTP/1.1 200 OK\r\nLocation: http://127.0.0.1:${socket.localPort}/multi-session\r\n" +
                        "Content-Length: 0\r\nConnection: close\r\n\r\n",
                )
                request.line.startsWith("PUT /multi-session ") -> {
                    val range = request.headers["content-range"]
                    contentRanges += range
                    if (contentRanges.size == 1) {
                        assertEquals("bytes 0-${chunkSize - 1}/$totalSize", range)
                        socket.respond(
                            "HTTP/1.1 308 Resume Incomplete\r\nRange: bytes=0-${chunkSize - 1}\r\n" +
                                "Content-Length: 0\r\nConnection: close\r\n\r\n",
                        )
                    } else {
                        assertEquals("bytes $chunkSize-${totalSize - 1}/$totalSize", range)
                        socket.respond(uploadSuccessResponse())
                    }
                }
                else -> error("Unexpected request: ${request.line}")
            }
        }
        val client = cioClientWithShortTimeouts()
        val file = File(tempDir, "multi-chunk.sav").apply { writeBytes(ByteArray(totalSize) { it.toByte() }) }

        try {
            val result = DriveTransfer(client, server.filesUrl, server.uploadFilesUrl).upload("token", file)
            server.awaitCompletion()

            assertEquals("cio-upload", result.id)
            assertEquals(
                listOf<String?>(
                    "bytes 0-${chunkSize - 1}/$totalSize",
                    "bytes $chunkSize-${totalSize - 1}/$totalSize",
                ),
                contentRanges,
            )
        } finally {
            client.close()
            server.close()
        }
    }

    @Test
    fun `CIO resumable upload restarts once after session expiry`(@TempDir tempDir: File) = runTest {
        var sessionCount = 0
        val uploadPaths = mutableListOf<String>()
        val server = LocalHttpServer(connectionCount = 4) { socket ->
            val request = socket.readRequest()
            when {
                request.line.startsWith("POST ") -> {
                    sessionCount++
                    socket.respond(
                        "HTTP/1.1 200 OK\r\nLocation: http://127.0.0.1:${socket.localPort}/session-$sessionCount\r\n" +
                            "Content-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                }
                request.line.startsWith("PUT /session-1 ") -> {
                    uploadPaths += request.line
                    socket.respond("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                }
                request.line.startsWith("PUT /session-2 ") -> {
                    uploadPaths += request.line
                    socket.respond(uploadSuccessResponse())
                }
                else -> error("Unexpected request: ${request.line}")
            }
        }
        val client = cioClientWithShortTimeouts()
        val file = File(tempDir, "expired-session.sav").apply { writeText("save") }

        try {
            val result = DriveTransfer(client, server.filesUrl, server.uploadFilesUrl).upload("token", file)
            server.awaitCompletion()

            assertEquals("cio-upload", result.id)
            assertEquals(2, sessionCount)
            assertEquals(listOf("PUT /session-1 HTTP/1.1", "PUT /session-2 HTTP/1.1"), uploadPaths)
        } finally {
            client.close()
            server.close()
        }
    }

    private fun cioClientWithShortTimeouts() = HttpClient(CIO) {
        install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
        install(HttpTimeout) {
            connectTimeoutMillis = 2_000
            requestTimeoutMillis = 150
            socketTimeoutMillis = 150
        }
    }

    private class LocalHttpServer(
        private val connectionCount: Int = 1,
        private val respond: (Socket) -> Unit,
    ) : AutoCloseable {
        private val socket = ServerSocket(0, 1, java.net.InetAddress.getByName("127.0.0.1"))
        private val failure = AtomicReference<Throwable?>()
        private val thread = Thread {
            try {
                repeat(connectionCount) {
                    socket.accept().use(respond)
                }
            } catch (error: Throwable) {
                if (!socket.isClosed) failure.set(error)
            }
        }.apply {
            isDaemon = true
            start()
        }

        val filesUrl: String = "http://127.0.0.1:${socket.localPort}/files"
        val uploadFilesUrl: String = "http://127.0.0.1:${socket.localPort}/upload/drive/v3/files"

        fun awaitCompletion() {
            thread.join(3_000)
            assertFalse(thread.isAlive, "The local HTTP response should finish")
            failure.get()?.let { throw AssertionError("Local HTTP server failed", it) }
        }

        override fun close() {
            socket.close()
            thread.join(3_000)
        }
    }

    private fun Socket.readRequestHeaders() {
        val reader = BufferedReader(InputStreamReader(getInputStream(), StandardCharsets.US_ASCII))
        while (reader.readLine()?.isNotEmpty() == true) Unit
    }

    private data class LocalRequest(val line: String, val headers: Map<String, String>)

    private fun Socket.readRequest(): LocalRequest {
        val input = getInputStream()
        val headerBytes = ByteArrayOutputStream()
        var delimiterMatches = 0
        val delimiter = byteArrayOf('\r'.code.toByte(), '\n'.code.toByte(), '\r'.code.toByte(), '\n'.code.toByte())
        while (delimiterMatches < delimiter.size) {
            val next = input.read()
            if (next == -1) error("Connection closed before request headers were complete")
            headerBytes.write(next)
            delimiterMatches = if (next == delimiter[delimiterMatches].toInt()) {
                delimiterMatches + 1
            } else if (next == delimiter[0].toInt()) {
                1
            } else {
                0
            }
        }
        val headerLines = headerBytes.toString(StandardCharsets.US_ASCII).split("\r\n")
        val headers = headerLines.drop(1).mapNotNull { line ->
            val separator = line.indexOf(':')
            if (separator < 0) null else line.substring(0, separator).lowercase() to line.substring(separator + 1).trim()
        }.toMap()
        val contentLength = headers["content-length"]?.toIntOrNull() ?: 0
        var remaining = contentLength
        val bodyBuffer = ByteArray(16 * 1024)
        while (remaining > 0) {
            val read = input.read(bodyBuffer, 0, minOf(bodyBuffer.size, remaining))
            if (read == -1) error("Connection closed before request body was complete")
            remaining -= read
        }
        return LocalRequest(headerLines.first(), headers)
    }

    private fun Socket.respond(response: String) {
        getOutputStream().apply {
            write(response.toByteArray(StandardCharsets.US_ASCII))
            flush()
        }
    }

    private fun uploadSuccessResponse(): String {
        val body = "{\"id\":\"cio-upload\",\"name\":\"recovered.sav\",\"modifiedTime\":\"2026-03-18T12:00:00Z\"}"
        return "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n" +
            "Content-Length: ${body.toByteArray(StandardCharsets.US_ASCII).size}\r\n" +
            "Connection: close\r\n\r\n$body"
    }

}
