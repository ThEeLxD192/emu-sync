package com.emusync.drive

import io.ktor.client.HttpClient
import io.ktor.client.engine.cio.CIO
import io.ktor.client.plugins.HttpTimeout
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.BufferedReader
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

    private fun cioClientWithShortTimeouts() = HttpClient(CIO) {
        install(HttpTimeout) {
            connectTimeoutMillis = 1_000
            requestTimeoutMillis = 40
            socketTimeoutMillis = 40
        }
    }

    private class LocalHttpServer(
        private val respond: (Socket) -> Unit,
    ) : AutoCloseable {
        private val socket = ServerSocket(0, 1, java.net.InetAddress.getByName("127.0.0.1"))
        private val failure = AtomicReference<Throwable?>()
        private val thread = Thread {
            try {
                socket.accept().use(respond)
            } catch (error: Throwable) {
                if (!socket.isClosed) failure.set(error)
            }
        }.apply {
            isDaemon = true
            start()
        }

        val filesUrl: String = "http://127.0.0.1:${socket.localPort}/files"

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
}
