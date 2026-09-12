package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.model.AppConfig
import com.emusync.model.EmulatorSystem
import com.emusync.model.GoogleDriveConfig
import com.emusync.ui.AppStatus
import com.emusync.ui.CloudSyncStatus
import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.respondOk
import io.ktor.http.HttpHeaders
import io.ktor.http.headersOf
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.serialization.kotlinx.json.json
import kotlinx.serialization.json.Json
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import java.time.Instant
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class DirectorySyncTest {

    private val jsonHeaders = headersOf(HttpHeaders.ContentType, "application/json")

    private fun createMockDriveClient(
        cloudFiles: List<Pair<String, String>>, // Pair(fileName, modifiedTimeIso)
        fileContents: Map<String, ByteArray> = emptyMap(),
        uploadResponseModTime: String = "2026-06-01T15:00:00.000Z",
    ): HttpClient {
        return HttpClient(MockEngine) {
            install(ContentNegotiation) {
                json(Json { ignoreUnknownKeys = true })
            }
            engine {
                addHandler { request ->
                    val url = request.url.toString()
                    val q = request.url.parameters["q"] ?: ""
                    when {
                        // OAuth token refresh
                        url.contains("oauth2.googleapis.com/token") -> {
                            respond(
                                content = """{"access_token":"mock-token","expires_in":3600,"token_type":"Bearer"}""",
                                headers = jsonHeaders,
                            )
                        }
                        // Upload or update file
                        url.contains("upload/drive/v3/files") -> {
                            respond(
                                content = """{"id":"mock-uploaded-id","name":"mock-file","modifiedTime":"$uploadResponseModTime"}""",
                                headers = jsonHeaders,
                            )
                        }
                        // Root folder search: EmuSync
                        q.contains("name = 'EmuSync'") -> {
                            respond(
                                content = """{"files":[{"id":"root-emusync","name":"EmuSync"}]}""",
                                headers = jsonHeaders,
                            )
                        }
                        // Entry folder search (e.g. Playstation 3)
                        q.contains("mimeType = 'application/vnd.google-apps.folder'") -> {
                            respond(
                                content = """{"files":[{"id":"folder-entry","name":"Playstation 3"}]}""",
                                headers = jsonHeaders,
                            )
                        }
                        // Download file media
                        request.url.parameters["alt"] == "media" -> {
                            val fileId = request.url.segments.last()
                            val bytes = fileContents[fileId] ?: "DUMMY".toByteArray()
                            respond(bytes, headers = headersOf(HttpHeaders.ContentType, "application/octet-stream"))
                        }
                        // List files in folder
                        q.contains("in parents and trashed = false") -> {
                            val filesJson = cloudFiles.mapIndexed { idx, (name, modTime) ->
                                """{"id":"file-$idx","name":"$name","modifiedTime":"$modTime"}"""
                            }.joinToString(",")
                            respond(
                                content = """{"files":[$filesJson]}""",
                                headers = jsonHeaders,
                            )
                        }
                        else -> respond(
                            content = """{"files":[]}""",
                            headers = jsonHeaders,
                        )
                    }
                }
            }
        }
    }

    @Test
    fun `checkEntrySyncStatus should detect OUT_OF_SYNC when local folder has newer files`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)

        // Simulate PS3 savedata directory structure
        val ps3Savedata = File(tempDir, "savedata/BLUS30481").apply { mkdirs() }
        File(ps3Savedata, "PARAM.SFO").apply {
            writeText("PARAM_DATA")
            setLastModified(System.currentTimeMillis())
        }

        val entry = EmulatorSystem(
            name = "Playstation 3",
            executablePath = "rpcs3",
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("sfb"),
            savePaths = listOf(File(tempDir, "savedata").absolutePath),
        )

        val config = AppConfig(
            googleDrive = GoogleDriveConfig(
                clientId = "test-client",
                clientSecret = "test-secret",
                refreshToken = "test-refresh-token",
            ),
            entries = listOf(entry),
        )
        configManager.save(config)

        // Older cloud file timestamp (2 hours ago)
        val cloudTimestamp = "2024-01-01T10:00:00.000Z"
        val mockClient = createMockDriveClient(
            cloudFiles = listOf("BLUS30481/PARAM.SFO" to cloudTimestamp)
        )

        val orchestrator = SyncOrchestrator(
            client = mockClient,
            config = config,
            configManager = configManager,
            onStatus = {},
        )

        val status = orchestrator.checkEntrySyncStatus(entry)
        assertTrue(status == CloudSyncStatus.OUT_OF_SYNC || status == CloudSyncStatus.CONFLICT)
    }

    @Test
    fun `checkEntrySyncStatus should return IN_SYNC when local and cloud timestamps match within threshold`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)

        val ps3Savedata = File(tempDir, "savedata/BLUS30481").apply { mkdirs() }
        val paramSfo = File(ps3Savedata, "PARAM.SFO").apply {
            writeText("PARAM_DATA")
        }

        val nowIso = Instant.ofEpochMilli(paramSfo.lastModified()).toString()

        val entry = EmulatorSystem(
            name = "Playstation 3",
            executablePath = "rpcs3",
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("sfb"),
            savePaths = listOf(File(tempDir, "savedata").absolutePath),
        )

        val config = AppConfig(
            googleDrive = GoogleDriveConfig(
                clientId = "test-client",
                clientSecret = "test-secret",
                refreshToken = "test-refresh-token",
            ),
            entries = listOf(entry),
        )
        configManager.save(config)

        val mockClient = createMockDriveClient(
            cloudFiles = listOf("BLUS30481/PARAM.SFO" to nowIso)
        )

        val orchestrator = SyncOrchestrator(
            client = mockClient,
            config = config,
            configManager = configManager,
            onStatus = {},
        )

        val status = orchestrator.checkEntrySyncStatus(entry)
        assertEquals(CloudSyncStatus.IN_SYNC, status)
    }

    @Test
    fun `checkEntrySyncStatus should return NOT_CONFIGURED when no refreshToken`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)

        val entry = EmulatorSystem(
            name = "Playstation 3",
            executablePath = "rpcs3",
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("sfb"),
            savePaths = listOf("/some/path"),
        )
        val config = AppConfig(googleDrive = null, entries = listOf(entry))
        configManager.save(config)

        val orchestrator = SyncOrchestrator(
            client = HttpClient(MockEngine) { engine { addHandler { respondOk() } } },
            config = config,
            configManager = configManager,
            onStatus = {},
        )

        val status = orchestrator.checkEntrySyncStatus(entry)
        assertEquals(CloudSyncStatus.NOT_CONFIGURED, status)
    }

    @Test
    fun `syncEntry manual download should preserve nested directory structure`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)

        val localSaveDir = File(tempDir, "savedata")

        val entry = EmulatorSystem(
            name = "Playstation 3",
            executablePath = "rpcs3",
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("sfb"),
            savePaths = listOf(localSaveDir.absolutePath),
        )

        val config = AppConfig(
            googleDrive = GoogleDriveConfig(
                clientId = "test-client",
                clientSecret = "test-secret",
                refreshToken = "test-refresh-token",
            ),
            entries = listOf(entry),
        )
        configManager.save(config)

        val mockClient = createMockDriveClient(
            cloudFiles = listOf(
                "BLUS30481/PARAM.SFO" to "2026-01-01T12:00:00.000Z",
                "BLUS30481/USRDIR/SYS-DATA" to "2026-01-01T12:00:00.000Z",
            ),
            fileContents = mapOf(
                "file-0" to "MOCK_PARAM_SFO_CONTENT".toByteArray(),
                "file-1" to "MOCK_SYS_DATA_CONTENT".toByteArray(),
            )
        )

        val statuses = mutableListOf<AppStatus>()
        val orchestrator = SyncOrchestrator(
            client = mockClient,
            config = config,
            configManager = configManager,
            onStatus = { statuses.add(it) },
        )

        val success = orchestrator.syncEntry(entry)
        assertTrue(success)

        // Verify that nested directories and files were created locally
        val downloadedSfo = File(localSaveDir, "BLUS30481/PARAM.SFO")
        val downloadedSys = File(localSaveDir, "BLUS30481/USRDIR/SYS-DATA")

        assertTrue(downloadedSfo.exists(), "PARAM.SFO should have been downloaded")
        assertEquals("MOCK_PARAM_SFO_CONTENT", downloadedSfo.readText())

        assertTrue(downloadedSys.exists(), "SYS-DATA should have been downloaded in nested USRDIR")
        assertEquals("MOCK_SYS_DATA_CONTENT", downloadedSys.readText())
    }

    @Test
    fun `syncEntry should prompt Conflict dialog on directory conflict and resolve to DOWNLOAD_CLOUD`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)

        val localSaveDir = File(tempDir, "savedata/BLUS30481").apply { mkdirs() }
        val localSfo = File(localSaveDir, "PARAM.SFO").apply {
            writeText("LOCAL_OLD_CONTENT")
            setLastModified(System.currentTimeMillis())
        }

        val entry = EmulatorSystem(
            name = "Playstation 3",
            executablePath = "rpcs3",
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("sfb"),
            savePaths = listOf(File(tempDir, "savedata").absolutePath),
        )

        val config = AppConfig(
            googleDrive = GoogleDriveConfig(
                clientId = "test-client",
                clientSecret = "test-secret",
                refreshToken = "test-refresh-token",
            ),
            entries = listOf(entry),
        )
        configManager.save(config)

        // Cloud file modified earlier than local -> causes CONFLICT
        val cloudModTime = "2024-01-01T10:00:00.000Z"
        val mockClient = createMockDriveClient(
            cloudFiles = listOf("BLUS30481/PARAM.SFO" to cloudModTime),
            fileContents = mapOf("file-0" to "CLOUD_OVERWRITE_CONTENT".toByteArray())
        )

        var conflictEncountered = false
        val orchestrator = SyncOrchestrator(
            client = mockClient,
            config = config,
            configManager = configManager,
            onStatus = { status ->
                if (status is AppStatus.Conflict) {
                    conflictEncountered = true
                    // Verify the displayed gameName is clean (not raw 'savedata' path)
                    assertEquals("Playstation 3", status.gameName)
                    status.onResolve(SyncDecision.DOWNLOAD_CLOUD)
                }
            },
        )

        val success = orchestrator.syncEntry(entry)
        assertTrue(success)
        assertTrue(conflictEncountered, "Conflict dialog should have been triggered for newer local files")
        assertEquals("CLOUD_OVERWRITE_CONTENT", localSfo.readText(), "Local file should have been overwritten with cloud content")
    }

    @Test
    fun `syncEntry manual download should preserve cloud modifiedTime on local files and subsequent check returns IN_SYNC`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)
        val localSaveDir = File(tempDir, "savedata")

        val entry = EmulatorSystem(
            name = "Playstation 3",
            executablePath = "rpcs3",
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("sfb"),
            savePaths = listOf(localSaveDir.absolutePath),
        )

        val config = AppConfig(
            googleDrive = GoogleDriveConfig(
                clientId = "test-client",
                clientSecret = "test-secret",
                refreshToken = "test-refresh-token",
            ),
            entries = listOf(entry),
        )
        configManager.save(config)

        val cloudTimestamp = "2026-01-01T12:00:00.000Z"
        val mockClient = createMockDriveClient(
            cloudFiles = listOf("BLUS30481/PARAM.SFO" to cloudTimestamp),
            fileContents = mapOf("file-0" to "MOCK_PARAM_SFO_CONTENT".toByteArray()),
        )

        val orchestrator = SyncOrchestrator(
            client = mockClient,
            config = config,
            configManager = configManager,
            onStatus = {},
        )

        val success = orchestrator.syncEntry(entry)
        assertTrue(success)

        val downloadedSfo = File(localSaveDir, "BLUS30481/PARAM.SFO")
        assertTrue(downloadedSfo.exists())
        assertEquals(Instant.parse(cloudTimestamp).toEpochMilli(), downloadedSfo.lastModified())

        val status = orchestrator.checkEntrySyncStatus(entry)
        assertEquals(CloudSyncStatus.IN_SYNC, status)
    }

    @Test
    fun `syncEntry manual upload should update local file timestamp to Drive response modifiedTime and subsequent check returns IN_SYNC`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)
        val localSaveDir = File(tempDir, "savedata/BLUS30481").apply { mkdirs() }
        val localSfo = File(localSaveDir, "PARAM.SFO").apply {
            writeText("LOCAL_CONTENT")
            setLastModified(1000000L)
        }

        val entry = EmulatorSystem(
            name = "Playstation 3",
            executablePath = "rpcs3",
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("sfb"),
            savePaths = listOf(File(tempDir, "savedata").absolutePath),
        )

        val config = AppConfig(
            googleDrive = GoogleDriveConfig(
                clientId = "test-client",
                clientSecret = "test-secret",
                refreshToken = "test-refresh-token",
            ),
            entries = listOf(entry),
        )
        configManager.save(config)

        val expectedDriveTimestamp = "2026-06-01T15:00:00.000Z"
        val mockClient = createMockDriveClient(
            cloudFiles = emptyList(),
            uploadResponseModTime = expectedDriveTimestamp,
        )

        val orchestrator = SyncOrchestrator(
            client = mockClient,
            config = config,
            configManager = configManager,
            onStatus = {},
        )

        val success = orchestrator.syncEntry(entry)
        assertTrue(success)

        val expectedEpochMs = Instant.parse(expectedDriveTimestamp).toEpochMilli()
        assertEquals(expectedEpochMs, localSfo.lastModified())

        val postUploadClient = createMockDriveClient(
            cloudFiles = listOf("BLUS30481/PARAM.SFO" to expectedDriveTimestamp),
        )
        val postUploadOrchestrator = SyncOrchestrator(
            client = postUploadClient,
            config = config,
            configManager = configManager,
            onStatus = {},
        )
        val status = postUploadOrchestrator.checkEntrySyncStatus(entry)
        assertEquals(CloudSyncStatus.IN_SYNC, status)
    }

    @Test
    fun `checkEntrySyncStatus should return IN_SYNC when directory has no local and no cloud files`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)
        val localSaveDir = File(tempDir, "savedata").apply { mkdirs() }

        val entry = EmulatorSystem(
            name = "Playstation 3",
            executablePath = "rpcs3",
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("sfb"),
            savePaths = listOf(localSaveDir.absolutePath),
        )

        val config = AppConfig(
            googleDrive = GoogleDriveConfig(
                clientId = "test-client",
                clientSecret = "test-secret",
                refreshToken = "test-refresh-token",
            ),
            entries = listOf(entry),
        )
        configManager.save(config)

        val mockClient = createMockDriveClient(
            cloudFiles = emptyList(),
        )

        val orchestrator = SyncOrchestrator(
            client = mockClient,
            config = config,
            configManager = configManager,
            onStatus = {},
        )

        val status = orchestrator.checkEntrySyncStatus(entry)
        assertEquals(CloudSyncStatus.IN_SYNC, status)
    }
}
