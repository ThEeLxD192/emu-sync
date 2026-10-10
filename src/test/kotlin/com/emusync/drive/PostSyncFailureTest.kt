package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.model.AppConfig
import com.emusync.model.EmulatorSystem
import com.emusync.model.GoogleDriveConfig
import com.emusync.ui.AppStatus
import com.emusync.ui.GameItem
import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import io.ktor.serialization.kotlinx.json.json
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import java.time.Instant
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertTrue

class PostSyncFailureTest {
    private val jsonHeaders = headersOf(HttpHeaders.ContentType, "application/json")

    @Test
    fun `partial post game upload reports successes and retries only failed saves`(@TempDir tempDir: File) = runTest {
        val rom = File(tempDir, "Game.rom").apply { writeText("ROM") }
        val save = File(tempDir, "save.dat").apply { writeText("LOCAL SAVE") }
        val secondSave = File(tempDir, "second.dat").apply { writeText("SECOND SAVE") }
        val entry = EmulatorSystem(
            name = "Test emulator",
            executablePath = "echo",
            arguments = listOf("{ROM}"),
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("rom"),
            group = "Tests",
            savePathsByRom = mapOf(rom.name to listOf(save.absolutePath, secondSave.absolutePath)),
        )
        val config = AppConfig(
            googleDrive = GoogleDriveConfig("client", "secret", "refresh"),
            entries = listOf(entry),
        )
        val configManager = ConfigManager(File(tempDir, "config.json").absolutePath)
        configManager.save(config)

        var uploadInitiations = 0
        var successfulUploads = 0
        val client = HttpClient(MockEngine) {
            install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
            engine {
                addHandler { request ->
                    val url = request.url.toString()
                    val query = request.url.parameters["q"].orEmpty()
                    when {
                        url.contains("oauth2.googleapis.com/token") -> respond(
                            """{"access_token":"token","expires_in":3600,"token_type":"Bearer"}""",
                            headers = jsonHeaders,
                        )
                        query.contains("name = 'EmuSync'") -> respond("""{"files":[{"id":"root","name":"EmuSync"}]}""", headers = jsonHeaders)
                        query.contains("name = 'Tests'") -> respond("""{"files":[{"id":"group","name":"Tests"}]}""", headers = jsonHeaders)
                        query.contains("name = 'Game'") -> respond("""{"files":[{"id":"game","name":"Game"}]}""", headers = jsonHeaders)
                        query.contains("in parents and trashed = false") -> respond("""{"files":[]}""", headers = jsonHeaders)
                        url.contains("upload/drive/v3/files") -> {
                            uploadInitiations++
                            if (uploadInitiations == 2) {
                                respond("offline", HttpStatusCode.InternalServerError)
                            } else {
                                respond("", headers = headersOf(HttpHeaders.Location, "https://uploads.example/session"))
                            }
                        }
                        url.contains("uploads.example/session") -> {
                            successfulUploads++
                            respond(
                                """{"id":"save-id","name":"save.dat","modifiedTime":"2026-10-09T12:00:00Z"}""",
                                headers = jsonHeaders,
                            )
                        }
                        else -> error("Unexpected request: ${request.method.value} $url")
                    }
                }
            }
        }
        val statuses = mutableListOf<AppStatus>()
        val orchestrator = SyncOrchestrator(client, config, configManager, statuses::add)

        orchestrator.playWithSync(GameItem("Game", entry, rom))

        val failure = assertIs<AppStatus.PostSyncFailed>(statuses.last())
        assertTrue(failure.reason.contains("500"))
        assertEquals(1, failure.completedSaveCount)
        assertEquals(1, failure.pendingSaveCount)
        assertEquals(1, successfulUploads)
        assertEquals("LOCAL SAVE", save.readText())
        assertEquals("SECOND SAVE", secondSave.readText())

        failure.onRetry()

        assertEquals(AppStatus.Idle, statuses.last())
        assertEquals(3, uploadInitiations)
        assertEquals(2, successfulUploads)
        assertEquals("LOCAL SAVE", save.readText())
        assertEquals("SECOND SAVE", secondSave.readText())
        assertEquals(1, statuses.count { it == AppStatus.Playing }, "Retry must not launch the game again")
    }

    @Test
    fun `directory retry does not force overwrite a cloud file updated after partial success`(@TempDir tempDir: File) = runTest {
        val rom = File(tempDir, "Game.rom").apply { writeText("ROM") }
        val saveDirectory = File(tempDir, "saves").apply { mkdirs() }
        val firstSave = File(saveDirectory, "first.dat").apply { writeText("FIRST") }
        val secondSave = File(saveDirectory, "second.dat").apply { writeText("SECOND") }
        val localTime = Instant.parse("2026-10-09T12:00:00Z").toEpochMilli()
        firstSave.setLastModified(localTime)
        secondSave.setLastModified(localTime)
        val entry = EmulatorSystem(
            name = "Test emulator",
            executablePath = "echo",
            arguments = listOf("{ROM}"),
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("rom"),
            group = "Tests",
            savePathsByRom = mapOf(rom.name to listOf(saveDirectory.absolutePath)),
        )
        val config = AppConfig(
            googleDrive = GoogleDriveConfig("client", "secret", "refresh"),
            entries = listOf(entry),
        )
        val configManager = ConfigManager(File(tempDir, "config.json").absolutePath).apply { save(config) }
        var firstCloudTime = "2020-01-01T00:00:00Z"
        var firstUpdates = 0
        var secondUpdates = 0
        var failSecondUpdate = true
        var failTokenRefresh = false
        val client = HttpClient(MockEngine) {
            install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
            engine {
                addHandler { request ->
                    val url = request.url.toString()
                    val query = request.url.parameters["q"].orEmpty()
                    when {
                        url.contains("oauth2.googleapis.com/token") && failTokenRefresh -> respond(
                            "offline", HttpStatusCode.InternalServerError,
                        )
                        url.contains("oauth2.googleapis.com/token") -> respond(
                            """{"access_token":"token","expires_in":3600,"token_type":"Bearer"}""", headers = jsonHeaders,
                        )
                        query.contains("name = 'EmuSync'") -> respond("""{"files":[{"id":"root","name":"EmuSync"}]}""", headers = jsonHeaders)
                        query.contains("name = 'Tests'") -> respond("""{"files":[{"id":"group","name":"Tests"}]}""", headers = jsonHeaders)
                        query.contains("name = 'Game'") -> respond("""{"files":[{"id":"game","name":"Game"}]}""", headers = jsonHeaders)
                        query.contains("in parents and trashed = false") -> respond(
                            """{"files":[{"id":"first-id","name":"first.dat","modifiedTime":"$firstCloudTime"},{"id":"second-id","name":"second.dat","modifiedTime":"2020-01-01T00:00:00Z"}]}""",
                            headers = jsonHeaders,
                        )
                        url.contains("upload/drive/v3/files/first-id") -> {
                            firstUpdates++
                            respond("", headers = headersOf(HttpHeaders.Location, "https://uploads.example/first"))
                        }
                        url.contains("upload/drive/v3/files/second-id") -> {
                            secondUpdates++
                            if (failSecondUpdate) respond("offline", HttpStatusCode.InternalServerError)
                            else respond("", headers = headersOf(HttpHeaders.Location, "https://uploads.example/second"))
                        }
                        url.contains("uploads.example/first") -> {
                            // Simulate another device publishing a newer copy after this upload.
                            firstCloudTime = "2027-01-01T00:00:00Z"
                            respond(
                                """{"id":"first-id","name":"first.dat","modifiedTime":"2026-10-09T12:00:00Z"}""",
                                headers = jsonHeaders,
                            )
                        }
                        url.contains("uploads.example/second") -> respond(
                            """{"id":"second-id","name":"second.dat","modifiedTime":"2026-10-09T12:00:00Z"}""",
                            headers = jsonHeaders,
                        )
                        else -> error("Unexpected request: ${request.method.value} $url")
                    }
                }
            }
        }
        val statuses = mutableListOf<AppStatus>()
        val orchestrator = SyncOrchestrator(client, config, configManager) { status ->
            statuses += status
            if (status is AppStatus.Conflict) status.onResolve(SyncDecision.UPLOAD_LOCAL)
        }

        orchestrator.playWithSync(GameItem("Game", entry, rom))

        val failure = assertIs<AppStatus.PostSyncFailed>(statuses.last())
        assertEquals(1, failure.completedSaveCount, failure.reason)
        assertEquals(1, failure.pendingSaveCount)
        assertEquals(1, firstUpdates)
        assertEquals(1, secondUpdates)
        assertEquals("FIRST", firstSave.readText())
        assertEquals("SECOND", secondSave.readText())

        failTokenRefresh = true
        failure.onRetry()

        val authorizationFailure = assertIs<AppStatus.PostSyncFailed>(statuses.last())
        assertTrue(authorizationFailure.reason.contains("500"))
        assertEquals(1, authorizationFailure.completedSaveCount)
        assertEquals(1, authorizationFailure.pendingSaveCount)
        assertEquals(1, firstUpdates)
        assertEquals(1, secondUpdates)
        assertEquals("FIRST", firstSave.readText())
        assertEquals("SECOND", secondSave.readText())

        failTokenRefresh = false
        failSecondUpdate = false
        authorizationFailure.onRetry()

        assertEquals(AppStatus.Idle, statuses.last())
        assertEquals(1, firstUpdates, "Retry must not force-update the file that already succeeded")
        assertEquals(2, secondUpdates)
        assertEquals("FIRST", firstSave.readText())
        assertEquals("SECOND", secondSave.readText())
        assertEquals(1, statuses.count { it == AppStatus.Playing }, "Retries must not launch the game again")
    }
}
