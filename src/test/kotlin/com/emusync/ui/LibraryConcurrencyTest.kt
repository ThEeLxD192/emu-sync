package com.emusync.ui

import com.emusync.config.ConfigManager
import com.emusync.drive.sha256
import com.emusync.drive.SaveMappingDrive
import com.emusync.model.*
import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.http.*
import io.ktor.serialization.kotlinx.json.json
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import kotlin.test.*

class LibraryConcurrencyTest {
    @Test
    fun `latest selection wins when scans complete in reverse order`(@TempDir directory: File) = runTest {
        val first = NativePCGame("First", "echo")
        val second = NativePCGame("Second", "echo")
        val firstScan = CompletableDeferred<List<GameItem>>()
        val secondScan = CompletableDeferred<List<GameItem>>()
        val manager = manager(directory, listOf(first, second))
        client().use { client ->
            val viewModel = AppViewModel(manager, httpClient = client, loadGames = {
                if (it == first) firstScan.await() else secondScan.await()
            })
            val loading = async { viewModel.loadConfig() }
            viewModel.uiState.first { it.selectedEntry == first }
            val selecting = async { viewModel.selectEntry(second) }
            viewModel.uiState.first { it.selectedEntry == second }
            assertTrue(viewModel.uiState.value.gameItems.isEmpty())
            secondScan.complete(listOf(GameItem("new", second)))
            selecting.await()
            firstScan.complete(listOf(GameItem("stale", first)))
            loading.await()
            assertEquals(second, viewModel.uiState.value.selectedEntry)
            assertEquals(listOf("new"), viewModel.uiState.value.gameItems.map { it.name })
            assertFalse(viewModel.uiState.value.isLoading)
        }
    }

    @Test
    fun `cancelled old scan cannot clear newer loading state`(@TempDir directory: File) = runTest {
        val first = NativePCGame("First", "echo")
        val second = NativePCGame("Second", "echo")
        val manager = manager(directory, listOf(first, second))
        val ready = CompletableDeferred<List<GameItem>>()
        client().use { client ->
            var firstLoad = true
            val viewModel = AppViewModel(manager, httpClient = client, loadGames = {
                if (firstLoad) { firstLoad = false; listOf(GameItem(it.name, it)) }
                else if (it == first) awaitCancellation() else ready.await()
            })
            viewModel.loadConfig()
            val old = async { viewModel.selectEntry(first) }
            viewModel.uiState.first { it.isLoading }
            val latest = async { viewModel.selectEntry(second) }
            viewModel.uiState.first { it.selectedEntry == second }
            old.cancelAndJoin()
            assertTrue(viewModel.uiState.value.isLoading)
            assertEquals(second, viewModel.uiState.value.selectedEntry)
            latest.cancelAndJoin()
            assertFalse(viewModel.uiState.value.isLoading)
        }
    }

    @Test
    fun `deleting a category invalidates its unfinished scan`(@TempDir directory: File) = runTest {
        val game = launchableGame()
        val manager = manager(directory, listOf(game))
        val scan = CompletableDeferred<List<GameItem>>()
        client().use { client ->
            val viewModel = AppViewModel(manager, httpClient = client, loadGames = { scan.await() })
            val loading = async { viewModel.loadConfig() }
            viewModel.uiState.first { it.selectedEntry != null }
            viewModel.deleteEntry(game)
            scan.complete(listOf(GameItem("obsolete", game)))
            loading.await()
            assertNull(viewModel.uiState.value.selectedEntry)
            assertTrue(viewModel.uiState.value.gameItems.isEmpty())
            assertFalse(viewModel.uiState.value.isLoading)
        }
    }

    @Test
    fun `duplicate launches edits and syncs cannot replace a pending save setup`(@TempDir directory: File) = runTest {
        val game = launchableGame()
        val manager = manager(directory, listOf(game))
        client().use { client ->
            val viewModel = AppViewModel(manager, httpClient = client)
            viewModel.loadConfig()
            val playing = async { viewModel.launchGame(GameItem(game.name, game)) }
            val request = viewModel.uiState.first { it.saveSetupRequest != null }.saveSetupRequest!!
            assertTrue(viewModel.uiState.value.isOperationInProgress)
            viewModel.launchGame(GameItem(game.name, game))
            viewModel.syncEntryNow(game)
            assertFalse(viewModel.editEntry(game, game.copy(name = "Changed")))
            viewModel.deleteEntry(game)
            assertEquals(listOf(game), manager.load().entries)
            assertSame(request, viewModel.uiState.value.saveSetupRequest)
            viewModel.completeSaveSetup(emptyList(), request = request)
            playing.await()
            assertNull(viewModel.uiState.value.saveSetupRequest)
            assertFalse(viewModel.uiState.value.isOperationInProgress)
        }
    }

    @Test
    fun `cancelled setup releases the operation and old dialog cannot complete a new one`(@TempDir directory: File) = runTest {
        val game = launchableGame()
        val manager = manager(directory, listOf(game))
        client().use { client ->
            val viewModel = AppViewModel(manager, httpClient = client)
            viewModel.loadConfig()
            val first = async { viewModel.launchGame(GameItem(game.name, game)) }
            val old = viewModel.uiState.first { it.saveSetupRequest != null }.saveSetupRequest!!
            first.cancelAndJoin()
            assertNull(viewModel.uiState.value.saveSetupRequest)
            assertFalse(viewModel.uiState.value.isOperationInProgress)
            assertEquals(AppStatus.Idle, viewModel.uiState.value.status)
            val second = async { viewModel.launchGame(GameItem(game.name, game)) }
            val fresh = viewModel.uiState.first { it.saveSetupRequest != null }.saveSetupRequest!!
            assertNotSame(old, fresh)
            viewModel.completeSaveSetup(emptyList(), request = old)
            assertSame(fresh, viewModel.uiState.value.saveSetupRequest)
            assertTrue(second.isActive)
            viewModel.completeSaveSetup(emptyList(), request = fresh)
            second.await()
            assertFalse(viewModel.uiState.value.isOperationInProgress)
        }
    }

    @Test
    fun `retry is exclusive and a completed callback cannot be replayed`(@TempDir directory: File) = runTest {
        val state = MutableStateFlow(AppUiState())
        val operations = LibraryOperationCoordinator(state)
        val ready = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        var attempts = 0
        client().use { client ->
            val controller = controller(directory, state, client, operations)
            controller.publishStatus(AppStatus.PostSyncFailed("Game", "offline", 0, 1) {
                attempts++
                ready.complete(Unit)
                release.await()
                controller.publishStatus(AppStatus.Idle)
            })
            val retry = (state.value.status as AppStatus.PostSyncFailed).onRetry
            val first = async { retry() }
            ready.await()
            retry()
            assertEquals(1, attempts)
            assertTrue(operations.isBusy)
            release.complete(Unit)
            first.await()
            retry()
            assertEquals(1, attempts)
            assertEquals(AppStatus.Idle, state.value.status)
            assertFalse(state.value.isOperationInProgress)
        }
    }

    @Test
    fun `a failed action releases the coordinator and cancellation clears transient state`() = runTest {
        val state = MutableStateFlow(AppUiState())
        val operations = LibraryOperationCoordinator(state)
        assertFailsWith<IllegalStateException> { operations.runIfIdle { error("failure") } }
        assertFalse(operations.isBusy)
        assertFailsWith<CancellationException> { operations.runIfIdle {
            state.update { it.copy(status = AppStatus.Playing,
                entrySyncStatus = mapOf("Game" to CloudSyncStatus.SYNCING)) }
            throw CancellationException("cancelled")
        } }
        assertFalse(state.value.isOperationInProgress)
        assertEquals(AppStatus.Idle, state.value.status)
        assertEquals(CloudSyncStatus.IDLE, state.value.entrySyncStatus["Game"])
        assertEquals(42, operations.runIfIdle { 42 })
    }

    @Test
    fun `old status check cannot overwrite a newer status response`(@TempDir directory: File) = runTest {
        checkRace(directory, performSync = false)
    }

    @Test
    fun `status check started before a save operation cannot overwrite its result`(@TempDir directory: File) = runTest {
        checkRace(directory, performSync = true)
    }

    @Test
    fun `stale reorder events keep current entry details and newly added entries`(@TempDir directory: File) = runTest {
        val first = NativePCGame("First", "old")
        val second = NativePCGame("Second", "echo")
        val added = NativePCGame("Added elsewhere", "echo")
        val manager = manager(directory, listOf(first, second))
        client().use { client ->
            val viewModel = AppViewModel(manager, httpClient = client)
            viewModel.loadConfig()
            manager.update { it.copy(entries = listOf(first.copy(executablePath = "new"), second, added)) }
            viewModel.reorderEntriesList(listOf(second, first))
            assertEquals(listOf("Second", "First", added.name), manager.load().entries.map { it.name })
            assertEquals("new", (manager.load().entries[1] as NativePCGame).executablePath)
        }
    }

    @Test
    fun `editing a ROM from an old item keeps other ROM overrides`(@TempDir directory: File) = runTest {
        val firstRom = File(directory, "first.rom").apply { writeText("ROM") }
        val secondRom = File(directory, "second.rom").apply { writeText("ROM") }
        val entry = EmulatorSystem("Console", "echo", romsDirectory = directory.path, extensions = listOf("rom"))
        val manager = manager(directory, listOf(entry))
        client().use { client ->
            val viewModel = AppViewModel(manager, httpClient = client)
            viewModel.loadConfig()
            viewModel.editGameOverride(GameItem("first", entry, firstRom), emptyList(), newCoverPath = "first.png")
            viewModel.editGameOverride(GameItem("second", entry, secondRom), emptyList(), newCoverPath = "second.png")
            val saved = manager.load().entries.single() as EmulatorSystem
            assertEquals(mapOf("first.rom" to "first.png", "second.rom" to "second.png"), saved.coverPathByRom)
        }
    }

    @Test
    fun `duplicate category names are rejected before changing configuration`(@TempDir directory: File) = runTest {
        val first = NativePCGame("First", "echo")
        val second = NativePCGame("Second", "echo")
        val manager = manager(directory, listOf(first, second))
        client().use { client ->
            val viewModel = AppViewModel(manager, httpClient = client)
            viewModel.loadConfig()
            viewModel.addEntry(first.copy(executablePath = "different"))
            assertEquals(2, manager.load().entries.size)
            assertFalse(viewModel.editEntry(second, second.copy(name = first.name)))
            assertEquals(listOf(first, second), manager.load().entries)
            assertIs<AppStatus.Error>(viewModel.uiState.value.status)
        }
    }

    @Test
    fun `renaming during initial restore uses the saved cloud folder for the active session`(@TempDir directory: File) = runTest {
        val game = launchableGame()
        val manager = ConfigManager(File(directory, "config.json")).apply {
            save(AppConfig(GoogleDriveConfig("client", "secret", "refresh"), listOf(game)))
        }
        SaveMappingDrive().use { drive ->
            drive.add("save", "save.dat", "CLOUD")
            val viewModel = AppViewModel(manager, httpClient = drive.client)
            viewModel.loadConfig()
            var folderAtDownload: String? = null
            drive.beforeDownload = { folderAtDownload = drive.folderQueries.last() }
            val launch = async { viewModel.launchGame(GameItem(game.name, game)) }
            val request = viewModel.uiState.first { it.saveSetupRequest != null }.saveSetupRequest!!
            val local = File(directory, "restored/save.dat")
            viewModel.completeSaveSetup(listOf(SaveTarget(local.path, SaveTargetType.FILE)), title = "Renamed", request = request)
            launch.await()
            assertEquals("Renamed", manager.load().entries.single().name)
            assertEquals("CLOUD", local.readText())
            assertTrue(folderAtDownload?.contains("name = 'Renamed'") == true, folderAtDownload)
            assertEquals(AppStatus.Idle, viewModel.uiState.value.status)
        }
    }

    @Test
    fun `failed initial setup does not launch with an unpersisted mapping`(@TempDir directory: File) = runTest {
        val game = launchableGame()
        val other = NativePCGame("Other", "echo")
        val manager = ConfigManager(File(directory, "config.json")).apply {
            save(AppConfig(GoogleDriveConfig("client", "secret", "refresh"), listOf(game, other)))
        }
        SaveMappingDrive().use { drive ->
            drive.add("save", "save.dat", "CLOUD")
            val viewModel = AppViewModel(manager, httpClient = drive.client)
            viewModel.loadConfig()
            val launch = async { viewModel.launchGame(GameItem(game.name, game)) }
            val request = viewModel.uiState.first { it.saveSetupRequest != null }.saveSetupRequest!!
            viewModel.completeSaveSetup(listOf(SaveTarget(File(directory, "save.dat").path, SaveTargetType.FILE)),
                title = "Other", request = request)
            launch.await()
            assertEquals(listOf(game, other), manager.load().entries)
            assertIs<AppStatus.Error>(viewModel.uiState.value.status)
            assertTrue(drive.downloads.isEmpty())
            assertFalse(File(directory, "save.dat").exists())
        }
    }

    private suspend fun CoroutineScope.checkRace(directory: File, performSync: Boolean) {
        val local = File(directory, "save.dat").apply { writeText("LOCAL") }
        val game = NativePCGame("Game", "echo", saveTargets = listOf(SaveTarget(local.path, SaveTargetType.FILE)))
        val config = AppConfig(GoogleDriveConfig("client", "secret", "refresh"), listOf(game))
        val manager = ConfigManager(File(directory, "config.json")).apply { save(config) }
        val state = MutableStateFlow(AppUiState(config = config))
        val operations = LibraryOperationCoordinator(state)
        val firstRequest = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        var listed = 0
        val headers = headersOf(HttpHeaders.ContentType, "application/json")
        HttpClient(MockEngine { request ->
            val query = request.url.parameters["q"].orEmpty()
            when {
                request.url.host == "oauth2.googleapis.com" -> respond("""{"access_token":"token","expires_in":3600,"token_type":"Bearer"}""", headers = headers)
                query.contains("mimeType = 'application/vnd.google-apps.folder'") -> respond("""{"files":[{"id":"game","name":"Game"}]}""", headers = headers)
                else -> {
                    val ordinal = ++listed
                    if (ordinal == 1) { firstRequest.complete(Unit); release.await() }
                    val checksum = sha256((if (ordinal == 1) "OLD CLOUD" else "LOCAL").toByteArray())
                    respond("""{"files":[{"id":"save","name":"save.dat","modifiedTime":"2026-01-01T00:00:00Z","sha256Checksum":"$checksum"}]}""", headers = headers)
                }
            }
        }) { install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) } }.use { client ->
            val controller = controller(directory, state, client, operations)
            val old = async { controller.checkSyncForEntry(game) }
            firstRequest.await()
            if (performSync) controller.syncEntryNow(game) else controller.checkSyncForEntry(game)
            assertEquals(CloudSyncStatus.IN_SYNC, state.value.entrySyncStatus[game.name])
            release.complete(Unit)
            old.await()
            assertEquals(CloudSyncStatus.IN_SYNC, state.value.entrySyncStatus[game.name])
        }
    }

    private fun launchableGame() = NativePCGame("Game", File(System.getProperty("java.home"),
        "bin/${if (System.getProperty("os.name").lowercase().contains("windows")) "java.exe" else "java"}").path,
        arguments = listOf("-version"))

    private fun manager(directory: File, entries: List<GameEntry>) = ConfigManager(File(directory, "config.json")).apply {
        save(AppConfig(entries = entries))
    }
    private fun client() = HttpClient(MockEngine { respond("{}", headers = headersOf(HttpHeaders.ContentType, "application/json")) })
    private fun controller(directory: File, state: MutableStateFlow<AppUiState>, client: HttpClient,
        operations: LibraryOperationCoordinator) = LibraryController(state, ConfigManager(File(directory, "config.json")),
        client, operations, ::loadLibraryGames, {}, { _, _ -> })
}
