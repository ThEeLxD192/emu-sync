package com.emusync.ui

import com.emusync.update.UpdateInfo
import com.emusync.update.UpdateManager
import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.http.*
import io.ktor.serialization.kotlinx.json.json
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import kotlin.test.*

class UpdateConcurrencyTest {
    private val info = UpdateInfo("99.0.0", "v99.0.0", "update", "https://updates.example/download", 4, "EmuSync.AppImage")
    private val headers = headersOf(HttpHeaders.ContentType, "application/json")
    private val release = """{"tag_name":"v99.0.0","assets":[{"name":"EmuSync.AppImage","browser_download_url":"https://updates.example/download","size":4}]}"""

    @Test
    fun `late update check cannot replace a completed download`(@TempDir directory: File) = runTest {
        val state = MutableStateFlow(AppUiState())
        val checking = CompletableDeferred<Unit>()
        val releaseCheck = CompletableDeferred<Unit>()
        HttpClient(MockEngine { request ->
            if (request.url.host == "api.github.com") {
                checking.complete(Unit)
                releaseCheck.await()
                respond(release, headers = headers)
            } else respond("DATA", headers = headersOf(HttpHeaders.ContentType, "application/octet-stream"))
        }) { install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) } }.use { client ->
            val controller = UpdateController(state, manager(client, directory))
            val old = async { controller.checkForUpdates(manual = false) }
            checking.await()
            controller.download(info)
            val ready = assertIs<UpdateUiState.ReadyToRestart>(state.value.updateState)
            assertEquals("DATA", ready.downloadedFile.readText())
            releaseCheck.complete(Unit)
            old.await()
            assertSame(ready, state.value.updateState)
            controller.checkForUpdates(manual = false)
            assertSame(ready, state.value.updateState)
        }
    }

    @Test
    fun `duplicate download is ignored and a cancelled download can be retried`(@TempDir directory: File) = runTest {
        val state = MutableStateFlow(AppUiState())
        val downloading = CompletableDeferred<Unit>()
        val response = CompletableDeferred<Unit>()
        var requests = 0
        HttpClient(MockEngine {
            requests++
            downloading.complete(Unit)
            response.await()
            respond("DATA")
        }).use { client ->
            val controller = UpdateController(state, manager(client, directory))
            val first = async { controller.download(info) }
            downloading.await()
            controller.download(info)
            controller.checkForUpdates(manual = false)
            assertEquals(1, requests)
            assertIs<UpdateUiState.Downloading>(state.value.updateState)
            first.cancelAndJoin()
            assertIs<UpdateUiState.Available>(state.value.updateState)
            assertTrue(directory.listFiles()!!.isEmpty(), "Cancelled downloads must remove their staging files")
            response.complete(Unit)
            controller.download(info)
            assertEquals(2, requests)
            assertIs<UpdateUiState.ReadyToRestart>(state.value.updateState)
        }
    }

    @Test
    fun `release size mismatch cannot become a ready update`(@TempDir directory: File) = runTest {
        val state = MutableStateFlow(AppUiState())
        HttpClient(MockEngine { respond("SHORT") }).use { client ->
            UpdateController(state, manager(client, directory)).download(info)
            assertTrue(assertIs<UpdateUiState.Error>(state.value.updateState).message.contains("size"))
            assertTrue(directory.listFiles()!!.isEmpty())
        }
    }

    @Test
    fun `cancelled update check propagates cancellation and restores its previous state`(@TempDir directory: File) = runTest {
        val available = UpdateUiState.Available(info)
        val state = MutableStateFlow(AppUiState(updateState = available))
        val checking = CompletableDeferred<Unit>()
        HttpClient(MockEngine { checking.complete(Unit); awaitCancellation() }).use { client ->
            val controller = UpdateController(state, manager(client, directory))
            val job = async { controller.checkForUpdates(manual = true) }
            checking.await()
            job.cancelAndJoin()
            assertTrue(job.isCancelled)
            assertSame(available, state.value.updateState)
            assertTrue(state.value.showUpdateDialog)
        }
    }

    @Test
    fun `restart does not interrupt an active game session`(@TempDir directory: File) {
        val state = MutableStateFlow(AppUiState(isOperationInProgress = true))
        var restarts = 0
        val manager = object : UpdateManager() {
            override fun applyUpdateAndRestart(downloadedFile: File): Boolean { restarts++; return true }
        }
        assertFalse(UpdateController(state, manager).restart(File(directory, "update.AppImage")))
        assertEquals(0, restarts)
    }

    private fun manager(client: HttpClient, directory: File) = object : UpdateManager(client) {
        override fun isWindows() = false
        override fun getCurrentAppImagePath() = File(directory, "current.AppImage").path
        override fun isSteamGameMode() = false
    }
}
