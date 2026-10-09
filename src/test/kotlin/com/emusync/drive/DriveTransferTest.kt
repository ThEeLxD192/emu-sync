package com.emusync.drive

import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.respondError
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.assertThrows
import org.junit.jupiter.api.io.TempDir
import java.io.File
import java.time.Instant
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals

class DriveTransferTest {

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
}
