package com.emusync.update

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
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import java.io.File

class UpdateManagerTest {

    @Test
    fun `correctly compares semantic versions`() {
        assertTrue(UpdateManager.isNewerVersion("0.1.0", "0.1.1"))
        assertTrue(UpdateManager.isNewerVersion("0.1.0", "0.2.0"))
        assertTrue(UpdateManager.isNewerVersion("0.9.9", "1.0.0"))
        assertTrue(UpdateManager.isNewerVersion("0.1.0", "0.1.0.1"))

        assertFalse(UpdateManager.isNewerVersion("0.1.0", "0.1.0"))
        assertFalse(UpdateManager.isNewerVersion("0.1.1", "0.1.0"))
        assertFalse(UpdateManager.isNewerVersion("1.0.0", "0.9.9"))
        assertFalse(UpdateManager.isNewerVersion("0.2.0", "0.1.9"))
    }

    @Test
    fun `checkForUpdates detects newer release and AppImage asset`() = runTest {
        val mockEngine = MockEngine { request ->
            val jsonResponse = """
                {
                    "tag_name": "v0.3.0",
                    "name": "EmuSync 0.3.0",
                    "body": "Bug fixes and improvements",
                    "assets": [
                        {
                            "name": "EmuSync-x86_64.AppImage",
                            "browser_download_url": "https://example.com/download/EmuSync-x86_64.AppImage",
                            "size": 1024
                        }
                    ]
                }
            """.trimIndent()
            respond(
                content = jsonResponse,
                status = HttpStatusCode.OK,
                headers = headersOf(HttpHeaders.ContentType, "application/json")
            )
        }

        val client = HttpClient(mockEngine) {
            install(ContentNegotiation) {
                json(Json { ignoreUnknownKeys = true; isLenient = true })
            }
        }

        val manager = UpdateManager(client)
        val update = manager.checkForUpdates(currentVersion = "0.2.0")

        assertNotNull(update)
        assertEquals("0.3.0", update!!.version)
        assertEquals("v0.3.0", update.tagName)
        assertEquals("https://example.com/download/EmuSync-x86_64.AppImage", update.downloadUrl)
    }

    @Test
    fun `downloadUpdate streams bytes and reports progress`() = runTest {
        val sampleData = ByteArray(1024 * 10) { (it % 256).toByte() }
        val mockEngine = MockEngine {
            respond(
                content = sampleData,
                status = HttpStatusCode.OK,
                headers = headersOf(
                    HttpHeaders.ContentType to listOf("application/octet-stream"),
                    HttpHeaders.ContentLength to listOf(sampleData.size.toString())
                )
            )
        }

        val client = HttpClient(mockEngine)
        val manager = UpdateManager(client)
        val tempFile = File.createTempFile("mock-update", ".AppImage")

        try {
            var finalProgress = 0f
            val result = manager.downloadUpdate("https://example.com/download/file", tempFile) { progress ->
                finalProgress = progress
            }

            assertTrue(result.isSuccess)
            assertTrue(tempFile.exists())
            assertEquals(sampleData.size.toLong(), tempFile.length())
            assertEquals(1f, finalProgress)
        } finally {
            tempFile.delete()
        }
    }
}
