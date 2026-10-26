package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.model.AppConfig
import com.emusync.model.GoogleDriveConfig
import com.emusync.model.NativePCGame
import com.emusync.model.SaveTarget
import com.emusync.model.SaveTargetType
import com.emusync.ui.AppStatus
import com.emusync.ui.GameItem
import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.content.OutgoingContent
import io.ktor.http.headersOf
import io.ktor.serialization.kotlinx.json.json
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import java.time.Instant
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class PostPlayAuthorizationTest {
    @Test
    fun `post-play requests use a fresh token when the pre-play token is no longer accepted`(@TempDir directory: File) = runTest {
        checkPostPlayAuthorization(directory, unlinkDuringPlay = false)
    }

    @Test
    fun `unlink during play prevents uploading with the old connection`(@TempDir directory: File) = runTest {
        checkPostPlayAuthorization(directory, unlinkDuringPlay = true)
    }

    private suspend fun checkPostPlayAuthorization(directory: File, unlinkDuringPlay: Boolean) {
        val save = File(directory, "save.dat").apply { writeText("BEFORE PLAY") }
        val java = File(System.getProperty("java.home"), "bin/${if (System.getProperty("os.name").lowercase().contains("win")) "java.exe" else "java"}")
        val entry = NativePCGame("Game", java.path, listOf("-version"), saveTargets = listOf(SaveTarget(save.path, SaveTargetType.FILE)))
        val config = AppConfig(GoogleDriveConfig("client", "secret", "refresh"), listOf(entry))
        val manager = ConfigManager(File(directory, "config.json"))
        manager.save(config)
        var played = false
        var tokenRequests = 0
        var rejectedRequests = 0
        var uploaded: String? = null
        val headers = headersOf(HttpHeaders.ContentType, "application/json")
        val client = HttpClient(MockEngine { request ->
            if (request.url.host == "oauth2.googleapis.com") {
                tokenRequests++
                return@MockEngine respond(
                    """{"access_token":"${if (played) "after-play" else "before-play"}","expires_in":1,"token_type":"Bearer"}""",
                    headers = headers,
                )
            }
            if (request.headers[HttpHeaders.Authorization] != "Bearer ${if (played) "after-play" else "before-play"}") {
                rejectedRequests++
                return@MockEngine respond("Expired access token", HttpStatusCode.Unauthorized)
            }
            val query = request.url.parameters["q"].orEmpty()
            when {
                query.contains("mimeType = 'application/vnd.google-apps.folder'") -> respond(
                    """{"files":[{"id":"${if (query.contains("name = 'EmuSync'")) "root" else "game"}","name":"Game"}]}""", headers = headers,
                )
                query.contains("'game' in parents") -> respond("""{"files":[]}""", headers = headers)
                request.method.value == "POST" -> respond("", headers = headersOf(HttpHeaders.Location, "https://uploads.example/session"))
                request.method.value == "PUT" -> {
                    uploaded = (request.body as OutgoingContent.ByteArrayContent).bytes().decodeToString()
                    respond("""{"id":"save","name":"save.dat","modifiedTime":"${Instant.now()}"}""", headers = headers)
                }
                else -> error("Unexpected request: ${request.method} ${request.url}")
            }
        }) { install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) } }
        val statuses = mutableListOf<AppStatus>()
        try {
            val orchestrator = SyncOrchestrator(client, config, manager) { status ->
                statuses += status
                if (status == AppStatus.Playing) {
                    played = true
                    save.writeText("AFTER PLAY")
                    if (unlinkDuringPlay) manager.updateGoogleDrive { it?.copy(refreshToken = null) }
                }
            }

            orchestrator.playWithSync(GameItem("Game", entry))

            assertTrue(played)
            assertEquals(0, rejectedRequests, "A stale access token must not be used for post-game requests")
            if (unlinkDuringPlay) {
                assertEquals(1, tokenRequests)
                assertEquals(null, uploaded)
                assertEquals("AFTER PLAY", save.readText())
                assertTrue(statuses.last() is AppStatus.PostSyncFailed)
            } else {
                assertEquals(2, tokenRequests)
                assertEquals("AFTER PLAY", uploaded)
                assertEquals(AppStatus.Idle, statuses.last())
                assertFalse(statuses.any { it is AppStatus.PostSyncFailed })
            }
        } finally { client.close() }
    }
}
