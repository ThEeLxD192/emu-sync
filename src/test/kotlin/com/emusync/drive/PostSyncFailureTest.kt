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
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertTrue

class PostSyncFailureTest {
    private val jsonHeaders = headersOf(HttpHeaders.ContentType, "application/json")

    @Test
    fun `failed post game upload remains visible and can be retried without launching game`(@TempDir tempDir: File) = runTest {
        val rom = File(tempDir, "Game.rom").apply { writeText("ROM") }
        val save = File(tempDir, "save.dat").apply { writeText("LOCAL SAVE") }
        val entry = EmulatorSystem(
            name = "Test emulator",
            executablePath = "echo",
            arguments = listOf("{ROM}"),
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("rom"),
            group = "Tests",
            savePathsByRom = mapOf(rom.name to listOf(save.absolutePath)),
        )
        val config = AppConfig(
            googleDrive = GoogleDriveConfig("client", "secret", "refresh"),
            entries = listOf(entry),
        )
        val configManager = ConfigManager(File(tempDir, "config.json").absolutePath)
        configManager.save(config)

        var failUpload = true
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
                        url.contains("upload/drive/v3/files") && failUpload -> respond("offline", HttpStatusCode.InternalServerError)
                        url.contains("upload/drive/v3/files") -> respond(
                            "",
                            headers = headersOf(HttpHeaders.Location, "https://uploads.example/session"),
                        )
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
        assertEquals(0, successfulUploads)

        failUpload = false
        failure.onRetry()

        assertEquals(AppStatus.Idle, statuses.last())
        assertEquals(1, successfulUploads)
    }
}
