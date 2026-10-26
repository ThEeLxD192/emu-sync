package com.emusync.drive

import com.sun.net.httpserver.HttpServer
import io.ktor.client.HttpClient
import io.ktor.client.engine.cio.CIO
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.toByteArray
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.http.*
import io.ktor.serialization.kotlinx.json.json
import kotlinx.coroutines.*
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import java.net.InetSocketAddress
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlin.test.*

class ConditionalCloudWriteTest {
    private val time = "2026-10-01T12:00:00Z"
    private val headers = headersOf(HttpHeaders.ContentType, "application/json")
    private val original = sha256("ORIGINAL".toByteArray())
    private fun observed() = DriveFile("remote", "save.dat", time, version = "1", sha256Checksum = original)
    private fun metadata(etag: String? = "\"version-1\"", version: String? = "1", name: String = "save.dat", hash: String? = original) =
        ConditionalDriveFile("remote", name, time, etag, version, hash)
    private fun encode(file: ConditionalDriveFile) = Json.encodeToString(ConditionalDriveFile.serializer(), file)
    private fun client(engine: MockEngine) = HttpClient(engine) { install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) } }

    @Test
    fun `update streams all snapshot bytes and sends the exact strong precondition`(@TempDir directory: File) = runTest {
        val bytes = ByteArray(9 * 1024 * 1024) { (it % 239).toByte() }
        val file = File(directory, "save.dat").apply { writeBytes(bytes) }
        var puts = 0
        val progress = mutableListOf<TransferProgress>()
        client(MockEngine { request ->
            if (request.method == HttpMethod.Get) respond(encode(metadata()), headers = headers)
            else {
                puts++
                assertEquals(HttpMethod.Put, request.method)
                assertEquals("/upload/drive/v2/files/remote", request.url.encodedPath)
                assertEquals("media", request.url.parameters["uploadType"])
                assertEquals("\"version-1\"", request.headers[HttpHeaders.IfMatch])
                assertEquals(bytes.size.toLong(), request.body.contentLength)
                assertContentEquals(bytes, request.body.toByteArray())
                respond(encode(metadata(version = "2", hash = sha256(bytes))), headers = headers)
            }
        }).use { client ->
            val result = DriveConditionalUpdate(client, progress::add).update("token", observed(), file, "save.dat")
            assertEquals("2", result.version)
            assertEquals(1, puts)
            assertEquals(bytes.size.toLong(), progress.last().transferredBytes)
            assertEquals("save.dat", progress.last().fileName)
        }
    }

    @Test
    fun `missing weak or wildcard version tokens never authorize an update`(@TempDir directory: File) = runTest {
        val file = File(directory, "save.dat").apply { writeText("LOCAL") }
        for (etag in listOf(null, "", "*", "W/\"weak\"", "unquoted", "\"line\rbreak\"")) {
            var writes = 0
            client(MockEngine { request ->
                if (request.method == HttpMethod.Get) respond(encode(metadata(etag)), headers = headers)
                else { writes++; respond("should not upload") }
            }).use { client ->
                assertFailsWith<SaveSyncException> { DriveConditionalUpdate(client).update("token", observed(), file, "save.dat") }
                assertEquals(0, writes)
                assertEquals("LOCAL", file.readText())
            }
        }
    }

    @Test
    fun `changes before metadata check are rejected before sending save bytes`(@TempDir directory: File) = runTest {
        val file = File(directory, "save.dat").apply { writeText("LOCAL") }
        for (current in listOf(metadata(version = "2"), metadata(name = "renamed.dat"), metadata(hash = sha256("CLOUD".toByteArray())))) {
            var writes = 0
            client(MockEngine { request ->
                if (request.method == HttpMethod.Get) respond(encode(current), headers = headers)
                else { writes++; respond("should not upload") }
            }).use { client ->
                assertFailsWith<SaveSyncException> { DriveConditionalUpdate(client).update("token", observed(), file, "save.dat") }
                assertEquals(0, writes)
            }
        }
    }

    @Test
    fun `unverifiable observed metadata does not fall back to timestamps`(@TempDir directory: File) = runTest {
        val file = File(directory, "save.dat").apply { writeText("LOCAL") }
        var calls = 0
        client(MockEngine {
            calls++
            respond(encode(metadata(version = null, hash = null)), headers = headers)
        }).use { client ->
            assertFailsWith<SaveSyncException> {
                DriveConditionalUpdate(client).update("token", observed().copy(version = null, sha256Checksum = null), file, "save.dat")
            }
            assertEquals(1, calls)
        }
    }

    @Test
    fun `cloud precondition rejection is not retried unconditionally`(@TempDir directory: File) = runTest {
        val file = File(directory, "save.dat").apply { writeText("LOCAL") }
        var attempts = 0
        client(MockEngine { request ->
            if (request.method == HttpMethod.Get) respond(encode(metadata()), headers = headers)
            else { attempts++; respond("changed", HttpStatusCode.PreconditionFailed) }
        }).use { client ->
            val error = assertFailsWith<SaveSyncException> { DriveConditionalUpdate(client).update("token", observed(), file, "save.dat") }
            assertTrue(error.message!!.contains("another device"))
            assertEquals(1, attempts)
            assertEquals("LOCAL", file.readText())
        }
    }

    @Test
    fun `concurrent remote change keeps local cloud and previous synchronization history`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            drive.add("remote", "save.dat", "ORIGINAL")
            val local = File(directory, "save.dat").apply { writeText("ORIGINAL") }
            val history = SyncHistoryStore(File(directory, "config.json"))
            val sync = TrackedFileSync(DriveTransfer(drive.client), history)
            sync.remember(local, drive.files.getValue("remote"), "game", "save.dat")
            val key = history.key(local, "game", "save.dat")
            val previous = history.get(key)
            local.writeText("LOCAL")
            drive.beforeConditionalCommit = { drive.add("remote", "save.dat", "CLOUD") }
            assertFailsWith<SaveSyncException> { sync.upload("token", local, drive.files.getValue("remote"), "game", "save.dat") }
            assertEquals("LOCAL", local.readText())
            assertEquals("CLOUD", drive.contents.getValue("remote"))
            assertEquals(previous, history.get(key))
            assertEquals(SyncDecision.CONFLICT, sync.plan(local, drive.files.getValue("remote"), "game", "save.dat"))
            assertTrue(drive.uploadedNames.isEmpty())
        }
    }

    @Test
    fun `simultaneous initial creates retain both copies and divergent duplicates block selection`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            val first = File(directory, "first/save.dat").apply { parentFile.mkdirs(); writeText("FIRST") }
            val second = File(directory, "second/save.dat").apply { parentFile.mkdirs(); writeText("SECOND") }
            val history = SyncHistoryStore(File(directory, "config.json"))
            val sync = TrackedFileSync(DriveTransfer(drive.client), history)
            // Both devices observed an absent remote file before either creation.
            sync.upload("token", first, null, "game", "save.dat")
            sync.upload("token", second, null, "game", "save.dat")
            assertEquals(setOf("FIRST", "SECOND"), drive.contents.values.toSet())
            assertFailsWith<SaveSyncException> { canonicalCloudFiles(drive.files.values.toList()) }
            assertEquals(2, drive.files.size)
            assertEquals("FIRST", first.readText())
            assertEquals("SECOND", second.readText())
        }
    }

    @Test
    fun `identical duplicate content remains safe to compare but unknown content does not`() {
        val file = observed()
        assertEquals(1, canonicalCloudFiles(listOf(file, file.copy(id = "other"))).size)
        assertFailsWith<SaveSyncException> { canonicalCloudFiles(listOf(file, file.copy(id = "other", sha256Checksum = null))) }
    }

    @Test
    fun `two actual CIO writers with the same observed version have only one winner`(@TempDir directory: File) = runTest {
        val metadataReaders = CountDownLatch(2)
        val writes = AtomicInteger()
        val lock = Any()
        var version = 1
        var stored = "ORIGINAL".toByteArray()
        val executor = Executors.newCachedThreadPool()
        val server = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
        server.executor = executor
        server.createContext("/") { exchange ->
            fun reply(code: Int, value: String) {
                val bytes = value.toByteArray()
                exchange.responseHeaders.add("Content-Type", "application/json")
                exchange.sendResponseHeaders(code, bytes.size.toLong())
                exchange.responseBody.use { it.write(bytes) }
            }
            try {
                if (exchange.requestMethod == "GET") {
                    val captured = synchronized(lock) { encode(metadata("\"v$version\"", "$version", hash = sha256(stored))) }
                    metadataReaders.countDown()
                    check(metadataReaders.await(5, TimeUnit.SECONDS))
                    reply(200, captured)
                } else {
                    writes.incrementAndGet()
                    val bytes = exchange.requestBody.use { it.readBytes() }
                    synchronized(lock) {
                        if (exchange.requestHeaders.getFirst("If-Match") != "\"v$version\"") reply(412, "changed")
                        else {
                            stored = bytes
                            version++
                            reply(200, encode(metadata("\"v$version\"", "$version", hash = sha256(stored))))
                        }
                    }
                }
            } finally { exchange.close() }
        }
        server.start()
        val base = "http://127.0.0.1:${server.address.port}"
        val first = File(directory, "one.dat").apply { writeText("FIRST DEVICE") }
        val second = File(directory, "two.dat").apply { writeText("SECOND DEVICE") }
        HttpClient(CIO) { install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) } }.use { client ->
            try {
                val api = DriveConditionalUpdate(client, filesUrl = "$base/drive/v2/files", uploadUrl = "$base/upload/drive/v2/files")
                val results = listOf(first, second).map { file -> async(Dispatchers.IO) {
                    runCatching { api.update("token", observed(), file, "save.dat") }
                } }.awaitAll()
                assertEquals(1, results.count { it.isSuccess })
                assertEquals(1, results.count { it.exceptionOrNull() is SaveSyncException })
                val winningFile = listOf(first, second)[results.indexOfFirst { it.isSuccess }]
                assertContentEquals(winningFile.readBytes(), stored)
                assertEquals(2, writes.get())
                assertEquals(2, version)
                assertEquals("FIRST DEVICE", first.readText())
                assertEquals("SECOND DEVICE", second.readText())
            } finally { server.stop(0); executor.shutdownNow() }
        }
    }
}
