package com.emusync.ui

import com.emusync.domain.update.UpdateInfo
import com.emusync.update.DesktopUpdateInstaller
import com.emusync.update.GitHubUpdateSource
import com.emusync.update.UpdateStagingFiles
import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.http.HttpHeaders
import io.ktor.http.headersOf
import io.ktor.serialization.kotlinx.json.json
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertSame
import kotlin.test.assertTrue
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class UpdateConcurrencyTest {
    private val info =
        UpdateInfo(
            "99.0.0",
            "v99.0.0",
            "update",
            "https://updates.example/download",
            4,
            "EmuSync.AppImage",
        )
    private val headers = headersOf(HttpHeaders.ContentType, "application/json")
    private val release =
        """{"tag_name":"v99.0.0","assets":[{"name":"EmuSync.AppImage","browser_download_url":"https://updates.example/download","size":4}]}"""

    @Test
    fun `late update check cannot replace a completed download`(@TempDir directory: File) =
        runTest {
            val state = MutableStateFlow(AppUiState())
            val checking = CompletableDeferred<Unit>()
            val releaseCheck = CompletableDeferred<Unit>()
            HttpClient(
                    MockEngine { request ->
                        if (request.url.host == "api.github.com") {
                            checking.complete(Unit)
                            releaseCheck.await()
                            respond(release, headers = headers)
                        } else
                            respond(
                                "DATA",
                                headers =
                                    headersOf(HttpHeaders.ContentType, "application/octet-stream"),
                            )
                    }
                ) {
                    install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
                }
                .use { client ->
                    val controller = controller(state, client, directory)
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
    fun `duplicate download is ignored and a cancelled download can be retried`(
        @TempDir directory: File
    ) = runTest {
        val state = MutableStateFlow(AppUiState())
        val downloading = CompletableDeferred<Unit>()
        val response = CompletableDeferred<Unit>()
        var requests = 0
        HttpClient(
                MockEngine {
                    requests++
                    downloading.complete(Unit)
                    response.await()
                    respond("DATA")
                }
            )
            .use { client ->
                val controller = controller(state, client, directory)
                val first = async { controller.download(info) }
                downloading.await()
                controller.download(info)
                controller.checkForUpdates(manual = false)
                assertEquals(1, requests)
                assertIs<UpdateUiState.Downloading>(state.value.updateState)
                first.cancelAndJoin()
                assertIs<UpdateUiState.Available>(state.value.updateState)
                assertTrue(
                    directory.listFiles()!!.isEmpty(),
                    "Cancelled downloads must remove their staging files",
                )
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
            controller(state, client, directory).download(info)
            assertTrue(
                assertIs<UpdateUiState.Error>(state.value.updateState).message.contains("size")
            )
            assertTrue(directory.listFiles()!!.isEmpty())
        }
    }

    @Test
    fun `cancelled update check propagates cancellation and restores its previous state`(
        @TempDir directory: File
    ) = runTest {
        val available = UpdateUiState.Available(info)
        val state = MutableStateFlow(AppUiState(updateState = available))
        val checking = CompletableDeferred<Unit>()
        HttpClient(
                MockEngine {
                    checking.complete(Unit)
                    awaitCancellation()
                }
            )
            .use { client ->
                val controller = controller(state, client, directory)
                val job = async { controller.checkForUpdates(manual = true) }
                checking.await()
                job.cancelAndJoin()
                assertTrue(job.isCancelled)
                assertEquals(available, state.value.updateState)
                assertTrue(state.value.showUpdateDialog)
            }
    }

    @Test
    fun `restart does not interrupt an active game session`(@TempDir directory: File) {
        val state = MutableStateFlow(AppUiState(isOperationInProgress = true))
        var restarts = 0
        val manager =
            object : DesktopUpdateInstaller() {
                override fun restart(path: String): Boolean {
                    restarts++
                    return true
                }
            }
        HttpClient(MockEngine { error("No HTTP requests expected") }).use { client ->
            assertFalse(
                controller(state, client, directory, manager)
                    .restart(File(directory, "update.AppImage"))
            )
        }
        assertEquals(0, restarts)
    }

    private fun controller(
        state: MutableStateFlow<AppUiState>,
        client: HttpClient,
        directory: File,
        installer: DesktopUpdateInstaller =
            object : DesktopUpdateInstaller() {
                override fun isSteamGameMode() = false
            },
    ) =
        UpdateController(
            state,
            GitHubUpdateSource(client, "owner", "repo", "1.0.0", false),
            UpdateStagingFiles({ false }, { File(directory, "current.AppImage").path }),
            installer,
        )
}
