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
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
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
    fun `checkForUpdates detects newer release and AppImage asset on Linux`() = runTest {
        val mockEngine = MockEngine { _ ->
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
                        },
                        {
                            "name": "EmuSync-0.3.0.msi",
                            "browser_download_url": "https://example.com/download/EmuSync-0.3.0.msi",
                            "size": 2048
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

        val manager = object : UpdateManager(client) {
            override fun isWindows(): Boolean = false
        }
        val update = manager.checkForUpdates(currentVersion = "0.2.0")

        assertNotNull(update)
        assertEquals("0.3.0", update!!.version)
        assertEquals("v0.3.0", update.tagName)
        assertEquals("EmuSync-x86_64.AppImage", update.assetName)
        assertEquals("https://example.com/download/EmuSync-x86_64.AppImage", update.downloadUrl)
    }

    @Test
    fun `checkForUpdates detects newer release and MSI asset on Windows`() = runTest {
        val mockEngine = MockEngine { _ ->
            val jsonResponse = """
                {
                    "tag_name": "v0.3.0",
                    "name": "EmuSync 0.3.0",
                    "body": "Windows and Linux release",
                    "assets": [
                        {
                            "name": "EmuSync-x86_64.AppImage",
                            "browser_download_url": "https://example.com/download/EmuSync-x86_64.AppImage",
                            "size": 1024
                        },
                        {
                            "name": "EmuSync-0.3.0.msi",
                            "browser_download_url": "https://example.com/download/EmuSync-0.3.0.msi",
                            "size": 2048
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

        val manager = object : UpdateManager(client) {
            override fun isWindows(): Boolean = true
        }
        val update = manager.checkForUpdates(currentVersion = "0.2.0")

        assertNotNull(update)
        assertEquals("0.3.0", update!!.version)
        assertEquals("v0.3.0", update.tagName)
        assertEquals("EmuSync-0.3.0.msi", update.assetName)
        assertEquals("https://example.com/download/EmuSync-0.3.0.msi", update.downloadUrl)
    }

    @Test
    fun `findBestAssetForPlatform prioritizes MSI then EXE then ZIP on Windows`() {
        val appImage = GitHubAsset(name = "EmuSync.AppImage", downloadUrl = "http://appimage")
        val zipAsset = GitHubAsset(name = "EmuSync.zip", downloadUrl = "http://zip")
        val exeAsset = GitHubAsset(name = "EmuSync-Setup.exe", downloadUrl = "http://exe")
        val msiAsset = GitHubAsset(name = "EmuSync.msi", downloadUrl = "http://msi")

        // Priority 1: MSI
        val best1 = UpdateManager.findBestAssetForPlatform(listOf(zipAsset, appImage, exeAsset, msiAsset), isWindows = true)
        assertEquals("EmuSync.msi", best1?.name)

        // Priority 2: EXE
        val best2 = UpdateManager.findBestAssetForPlatform(listOf(zipAsset, appImage, exeAsset), isWindows = true)
        assertEquals("EmuSync-Setup.exe", best2?.name)

        // Priority 3: ZIP
        val best3 = UpdateManager.findBestAssetForPlatform(listOf(zipAsset, appImage), isWindows = true)
        assertEquals("EmuSync.zip", best3?.name)

        // No Windows asset
        val best4 = UpdateManager.findBestAssetForPlatform(listOf(appImage), isWindows = true)
        assertNull(best4)
    }

    @Test
    fun `findBestAssetForPlatform prioritizes AppImage then tar gz then deb on Linux`() {
        val appImage = GitHubAsset(name = "EmuSync.AppImage", downloadUrl = "http://appimage")
        val tarAsset = GitHubAsset(name = "EmuSync.tar.gz", downloadUrl = "http://tar")
        val debAsset = GitHubAsset(name = "EmuSync.deb", downloadUrl = "http://deb")
        val msiAsset = GitHubAsset(name = "EmuSync.msi", downloadUrl = "http://msi")

        // Priority 1: AppImage
        val best1 = UpdateManager.findBestAssetForPlatform(listOf(msiAsset, debAsset, tarAsset, appImage), isWindows = false)
        assertEquals("EmuSync.AppImage", best1?.name)

        // Priority 2: tar.gz
        val best2 = UpdateManager.findBestAssetForPlatform(listOf(msiAsset, debAsset, tarAsset), isWindows = false)
        assertEquals("EmuSync.tar.gz", best2?.name)

        // Priority 3: deb
        val best3 = UpdateManager.findBestAssetForPlatform(listOf(msiAsset, debAsset), isWindows = false)
        assertEquals("EmuSync.deb", best3?.name)

        // No Linux asset
        val best4 = UpdateManager.findBestAssetForPlatform(listOf(msiAsset), isWindows = false)
        assertNull(best4)
    }

    @Test
    fun `applyWindowsUpdateAndRestart launches msiexec for MSI files`(@TempDir tempDir: File) {
        val manager = UpdateManager()
        val msiFile = File(tempDir, "EmuSync-Setup.msi").apply { createNewFile() }

        var executedCommand: List<String>? = null
        val success = manager.applyWindowsUpdateAndRestart(msiFile) { cmd ->
            executedCommand = cmd
        }

        assertTrue(success)
        assertNotNull(executedCommand)
        assertEquals(listOf("msiexec", "/i", msiFile.absolutePath), executedCommand)
    }

    @Test
    fun `applyWindowsUpdateAndRestart launches EXE installer directly`(@TempDir tempDir: File) {
        val manager = UpdateManager()
        val exeFile = File(tempDir, "EmuSync-Setup.exe").apply { createNewFile() }

        var executedCommand: List<String>? = null
        val success = manager.applyWindowsUpdateAndRestart(exeFile) { cmd ->
            executedCommand = cmd
        }

        assertTrue(success)
        assertNotNull(executedCommand)
        assertEquals(listOf(exeFile.absolutePath), executedCommand)
    }

    @Test
    fun `applyWindowsUpdateAndRestart returns false for non-installer files`(@TempDir tempDir: File) {
        val manager = UpdateManager()
        val txtFile = File(tempDir, "readme.txt").apply { createNewFile() }

        var launched = false
        val success = manager.applyWindowsUpdateAndRestart(txtFile) { _ ->
            launched = true
        }

        assertFalse(success)
        assertFalse(launched)
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
