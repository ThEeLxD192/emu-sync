package com.emusync.update

import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.http.*
import kotlinx.coroutines.*
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import kotlin.test.*

class UpdateDownloadSafetyTest {
    @Test
    fun `truncated update retains an existing download and removes staging files`(@TempDir directory: File) = runTest {
        val destination = File(directory, "update.AppImage").apply { writeText("PREVIOUS VALID DOWNLOAD") }
        HttpClient(MockEngine { respond("SHORT", headers = headersOf(HttpHeaders.ContentLength, "100")) }).use { client ->
            val result = UpdateManager(client).downloadUpdate("https://updates.example/file", destination) {}
            assertTrue(result.isFailure)
            assertEquals("PREVIOUS VALID DOWNLOAD", destination.readText())
            assertEquals(listOf(destination.name), directory.listFiles()!!.map { it.name })
        }
    }

    @Test
    fun `empty and rejected downloads never replace a previous file`(@TempDir directory: File) = runTest {
        for (status in listOf(HttpStatusCode.OK, HttpStatusCode.ServiceUnavailable)) {
            val destination = File(directory, "update.AppImage").apply { writeText("PREVIOUS") }
            HttpClient(MockEngine { respond("", status = status) }).use { client ->
                assertTrue(UpdateManager(client).downloadUpdate("https://updates.example/file", destination) {}.isFailure)
                assertEquals("PREVIOUS", destination.readText())
            }
        }
        assertEquals(1, directory.listFiles()!!.size)
    }

    @Test
    fun `cancelled download propagates cancellation and preserves destination`(@TempDir directory: File) = runTest {
        val destination = File(directory, "update.AppImage").apply { writeText("PREVIOUS") }
        val requested = CompletableDeferred<Unit>()
        HttpClient(MockEngine { requested.complete(Unit); awaitCancellation() }).use { client ->
            val task = async { UpdateManager(client).downloadUpdate("https://updates.example/file", destination) {} }
            requested.await()
            task.cancelAndJoin()
            assertTrue(task.isCancelled)
            assertEquals("PREVIOUS", destination.readText())
            assertEquals(listOf(destination.name), directory.listFiles()!!.map { it.name })
        }
    }
}
