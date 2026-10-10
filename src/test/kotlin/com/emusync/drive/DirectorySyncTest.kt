package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.model.AppConfig
import com.emusync.model.EmulatorSystem
import com.emusync.model.GoogleDriveConfig
import com.emusync.model.NativePCGame
import com.emusync.model.SaveTarget
import com.emusync.model.SaveTargetType
import com.emusync.ui.AppStatus
import com.emusync.ui.CloudSyncStatus
import com.emusync.ui.GameItem
import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.respondOk
import io.ktor.http.HttpHeaders
import io.ktor.http.content.OutgoingContent
import io.ktor.http.headersOf
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.serialization.kotlinx.json.json
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import java.time.Instant
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class DirectorySyncTest {

    private val jsonHeaders = headersOf(HttpHeaders.ContentType, "application/json")

    private fun unknownDirectoryWithFileSibling(tempDir: File): Pair<NativePCGame, Pair<File, File>> {
        val directoryPath = File(tempDir, "missing/SAVE_SLOT")
        val siblingFilePath = File(tempDir, "other/SAVE_SLOT")
        val entry = NativePCGame(
            name = "Playstation 3",
            executablePath = "echo",
            arguments = listOf("test"),
            saveTargets = listOf(
                SaveTarget(directoryPath.absolutePath, SaveTargetType.UNKNOWN),
                SaveTarget(siblingFilePath.absolutePath, SaveTargetType.FILE),
            ),
        )
        return entry to (directoryPath to siblingFilePath)
    }

    private fun siblingCloudFiles() = listOf(
        "SAVE_SLOT" to "2026-01-01T12:00:00.000Z",
        "state-one.sav" to "2026-01-01T12:00:00.000Z",
        "state-two.sav" to "2026-01-01T12:00:00.000Z",
    )

    private fun siblingCloudContents() = mapOf(
        "file-0" to "SIBLING_FILE".toByteArray(),
        "file-1" to "DIRECTORY_SAVE_ONE".toByteArray(),
        "file-2" to "DIRECTORY_SAVE_TWO".toByteArray(),
    )

    private fun driveConfig() = AppConfig(
        googleDrive = GoogleDriveConfig(
            clientId = "test-client",
            clientSecret = "test-secret",
            refreshToken = "test-refresh-token",
        ),
    )

    @Test
    fun `play pre-sync separates unknown directory from same-name file sibling`(@TempDir tempDir: File) = runTest {
        val (entry, paths) = unknownDirectoryWithFileSibling(tempDir)
        val (directoryPath, siblingFilePath) = paths
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)
        val config = driveConfig().copy(entries = listOf(entry))
        val orchestrator = SyncOrchestrator(
            client = createMockDriveClient(siblingCloudFiles(), siblingCloudContents()),
            config = config,
            configManager = configManager,
            onStatus = {},
        )

        orchestrator.playWithSync(GameItem(name = entry.name, entry = entry))

        assertEquals("SIBLING_FILE", siblingFilePath.readText())
        assertEquals("DIRECTORY_SAVE_ONE", File(directoryPath, "state-one.sav").readText())
        assertEquals("DIRECTORY_SAVE_TWO", File(directoryPath, "state-two.sav").readText())
        assertFalse(File(directoryPath, "SAVE_SLOT").exists())
    }

    @Test
    fun `manual sync separates unknown directory from same-name file sibling`(@TempDir tempDir: File) = runTest {
        val (entry, paths) = unknownDirectoryWithFileSibling(tempDir)
        val (directoryPath, siblingFilePath) = paths
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)
        val config = driveConfig().copy(entries = listOf(entry))
        val orchestrator = SyncOrchestrator(
            client = createMockDriveClient(siblingCloudFiles(), siblingCloudContents()),
            config = config,
            configManager = configManager,
            onStatus = {},
        )

        assertTrue(orchestrator.syncEntry(entry))

        assertEquals("SIBLING_FILE", siblingFilePath.readText())
        assertEquals("DIRECTORY_SAVE_ONE", File(directoryPath, "state-one.sav").readText())
        assertEquals("DIRECTORY_SAVE_TWO", File(directoryPath, "state-two.sav").readText())
        assertFalse(File(directoryPath, "SAVE_SLOT").exists())
    }

    @Test
    fun `migrated per ROM unknown directory restores before play and uploads after play`(@TempDir tempDir: File) = runTest {
        val romFile = File(tempDir, "Game.rom").apply { writeText("ROM") }
        val saveDirectory = File(tempDir, "not-created-yet/profile.d")
        val javaExecutable = File(
            System.getProperty("java.home"),
            "bin/${if (System.getProperty("os.name").lowercase().contains("win")) "java.exe" else "java"}",
        ).absolutePath
        val configFile = File(tempDir, "config.json").apply {
            writeText(
                """
                {
                  "googleDrive": {
                    "clientId": "test-client",
                    "clientSecret": "test-secret",
                    "refreshToken": "test-refresh-token"
                  },
                  "entries": [{
                    "type": "emulator",
                    "name": "Legacy emulator",
                    "executablePath": ${Json.encodeToString(javaExecutable)},
                    "arguments": ["-version"],
                    "romsDirectory": ${Json.encodeToString(tempDir.absolutePath)},
                    "extensions": ["rom"],
                    "group": "Legacy system",
                    "savePathsByRom": {
                      "${romFile.name}": [${Json.encodeToString(saveDirectory.absolutePath)}]
                    }
                  }]
                }
                """.trimIndent(),
            )
        }
        val configManager = ConfigManager(configFile)
        val migratedConfig = configManager.load()
        val entry = migratedConfig.entries.single() as EmulatorSystem

        assertEquals(
            SaveTargetType.UNKNOWN,
            entry.saveTargetsByRom[romFile.name]?.single()?.type,
            "A missing legacy per-ROM target should remain UNKNOWN after migration",
        )

        val cloudFiles = listOf(
            "cloud-initial" to ("profiles/slot1/save.dat" to "2026-09-25T15:00:00.000Z"),
            "cloud-options" to ("profiles/slot2/options.ini" to "2026-09-25T15:00:00.000Z"),
        )
        val cloudContents = mapOf(
            "cloud-initial" to "CLOUD_INITIAL".toByteArray(),
            "cloud-options" to "CLOUD_OPTIONS".toByteArray(),
        )
        val uploadRequests = mutableListOf<String>()
        var currentUploadName = ""
        val mockClient = HttpClient(MockEngine) {
            install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
            engine {
                addHandler { request ->
                    val url = request.url.toString()
                    val q = request.url.parameters["q"] ?: ""
                    when {
                        url.contains("oauth2.googleapis.com/token") -> respond(
                            content = """{"access_token":"mock-token","expires_in":3600,"token_type":"Bearer"}""",
                            headers = jsonHeaders,
                        )
                        q.contains("name = 'EmuSync'") -> respond(
                            content = """{"files":[{"id":"root-id","name":"EmuSync"}]}""", headers = jsonHeaders,
                        )
                        q.contains("name = 'Legacy system'") -> respond(
                            content = """{"files":[{"id":"system-id","name":"Legacy system"}]}""", headers = jsonHeaders,
                        )
                        q.contains("name = 'Game'") -> respond(
                            content = """{"files":[{"id":"game-id","name":"Game"}]}""", headers = jsonHeaders,
                        )
                        request.url.parameters["alt"] == "media" -> {
                            val fileId = request.url.segments.last()
                            respond(
                                cloudContents.getValue(fileId),
                                headers = headersOf(HttpHeaders.ContentType, "application/octet-stream"),
                            )
                        }
                        q.contains("'game-id' in parents") -> {
                            val filesJson = cloudFiles.joinToString(",") { (id, details) ->
                                val (name, modifiedTime) = details
                                """{"id":"$id","name":"$name","modifiedTime":"$modifiedTime"}"""
                            }
                            respond(content = """{"files":[$filesJson]}""", headers = jsonHeaders)
                        }
                        url.contains("upload/drive/v3/files") && request.method.value in setOf("POST", "PATCH") -> {
                            if (request.method.value == "PATCH") {
                                uploadRequests += "PATCH:${request.url.segments.last()}"
                                currentUploadName = "profiles/slot1/save.dat"
                            } else {
                                val metadata = Json.parseToJsonElement(
                                    (request.body as OutgoingContent.ByteArrayContent).bytes().decodeToString(),
                                ).jsonObject
                                currentUploadName = metadata["name"]?.jsonPrimitive?.content ?: ""
                                uploadRequests += "POST:$currentUploadName"
                            }
                            respond(
                                content = "",
                                headers = headersOf(HttpHeaders.Location, "https://uploads.example/legacy-per-rom"),
                            )
                        }
                        request.method.value == "PUT" -> respond(
                            content = """{"id":"uploaded-id","name":"$currentUploadName","modifiedTime":"2026-10-09T15:00:00.000Z"}""",
                            headers = jsonHeaders,
                        )
                        else -> respond(content = """{"files":[]}""", headers = jsonHeaders)
                    }
                }
            }
        }

        val preSyncRestored = mutableListOf<String>()
        val orchestrator = SyncOrchestrator(
            client = mockClient,
            config = migratedConfig,
            configManager = configManager,
            onStatus = { status ->
                if (status == AppStatus.Playing) {
                    val restored = File(saveDirectory, "profiles/slot1/save.dat")
                    preSyncRestored += restored.takeIf { it.exists() }?.readText() ?: "MISSING"
                    File(saveDirectory, "profiles/slot2/options.ini").apply {
                        if (exists()) preSyncRestored += readText() else preSyncRestored += "MISSING_OPTIONS"
                    }
                    restored.writeText("UPDATED_DURING_PLAY")
                    restored.setLastModified(Instant.now().plusSeconds(30).toEpochMilli())
                    File(saveDirectory, "profiles/new-slot/new.sav").apply {
                        parentFile.mkdirs()
                        writeText("CREATED_DURING_PLAY")
                    }
                }
            },
        )

        orchestrator.playWithSync(
            GameItem(name = romFile.nameWithoutExtension, entry = entry, romFile = romFile),
        )

        assertEquals(listOf("CLOUD_INITIAL", "CLOUD_OPTIONS"), preSyncRestored)
        assertEquals("UPDATED_DURING_PLAY", File(saveDirectory, "profiles/slot1/save.dat").readText())
        assertEquals("CLOUD_OPTIONS", File(saveDirectory, "profiles/slot2/options.ini").readText())
        assertEquals("CREATED_DURING_PLAY", File(saveDirectory, "profiles/new-slot/new.sav").readText())
        assertEquals(
            setOf("PATCH:cloud-initial", "POST:profiles/new-slot/new.sav"),
            uploadRequests.toSet(),
            "Post-sync should update and create files using relative paths from the UNKNOWN directory target",
        )
    }

    @Test
    fun `post-sync does not update file sibling with colliding directory save`(@TempDir tempDir: File) = runTest {
        val directoryPath = File(tempDir, "saves/SAVE_SLOT").apply { mkdirs() }
        val conflictingDirectorySave = File(directoryPath, "SAVE_SLOT").apply {
            writeText("LOCAL_DIRECTORY_SAVE")
            setLastModified(System.currentTimeMillis())
        }
        val siblingFilePath = File(tempDir, "other/SAVE_SLOT")
        val entry = NativePCGame(
            name = "Playstation 3",
            executablePath = "echo",
            arguments = listOf("test"),
            saveTargets = listOf(
                SaveTarget(directoryPath.absolutePath, SaveTargetType.DIRECTORY),
                SaveTarget(siblingFilePath.absolutePath, SaveTargetType.FILE),
            ),
        )
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)
        val config = driveConfig().copy(entries = listOf(entry))
        var writeCount = 0
        val statuses = mutableListOf<AppStatus>()
        val orchestrator = SyncOrchestrator(
            client = createMockDriveClient(
                cloudFiles = listOf("SAVE_SLOT" to "2026-01-01T12:00:00.000Z"),
                fileContents = mapOf("file-0" to "SIBLING_FILE".toByteArray()),
                onWrite = { writeCount++ },
            ),
            config = config,
            configManager = configManager,
            onStatus = statuses::add,
        )

        orchestrator.playWithSync(GameItem(name = entry.name, entry = entry))

        assertEquals("SIBLING_FILE", siblingFilePath.readText())
        assertEquals("LOCAL_DIRECTORY_SAVE", conflictingDirectorySave.readText())
        assertEquals(0, writeCount, "The colliding directory file must not overwrite the sibling Drive file")
        assertTrue(
            statuses.any { it is AppStatus.Error && it.message.contains("Rename or reconfigure") },
            "A deterministic name collision should explain how to resolve it",
        )
        assertFalse(
            statuses.any { it is AppStatus.PostSyncFailed },
            "A deterministic collision should not offer a retry that must fail again",
        )
    }

    @Test
    fun `migrated missing per ROM extensionless file restores exact Drive match before play`(@TempDir tempDir: File) = runTest {
        val romFile = File(tempDir, "Game.rom").apply { writeText("ROM") }
        val legacySave = File(tempDir, "not-created-yet/SAVE_SLOT")
        val javaExecutable = File(
            System.getProperty("java.home"),
            "bin/${if (System.getProperty("os.name").lowercase().contains("win")) "java.exe" else "java"}",
        ).absolutePath
        val configFile = File(tempDir, "config.json").apply {
            writeText(
                """
                {
                  "googleDrive": {
                    "clientId": "test-client",
                    "clientSecret": "test-secret",
                    "refreshToken": "test-refresh-token"
                  },
                  "entries": [{
                    "type": "emulator",
                    "name": "Legacy emulator",
                    "executablePath": ${Json.encodeToString(javaExecutable)},
                    "arguments": ["-version"],
                    "romsDirectory": ${Json.encodeToString(tempDir.absolutePath)},
                    "extensions": ["rom"],
                    "group": "Legacy system",
                    "savePathsByRom": {
                      "${romFile.name}": [${Json.encodeToString(legacySave.absolutePath)}]
                    }
                  }]
                }
                """.trimIndent(),
            )
        }
        val configManager = ConfigManager(configFile)
        val migratedConfig = configManager.load()
        val entry = migratedConfig.entries.single() as EmulatorSystem

        assertEquals(
            SaveTargetType.UNKNOWN,
            entry.saveTargetsByRom[romFile.name]?.single()?.type,
            "A missing extensionless legacy per-ROM path should stay UNKNOWN through migration",
        )
        assertFalse(legacySave.exists())

        val cloudModifiedTime = "2026-09-25T15:00:00.000Z"
        val uploadRequests = mutableListOf<String>()
        var sawRestoredFileBeforePlay = false
        var currentUploadName = "SAVE_SLOT"
        val mockClient = HttpClient(MockEngine) {
            install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
            engine {
                addHandler { request ->
                    val url = request.url.toString()
                    val q = request.url.parameters["q"] ?: ""
                    when {
                        url.contains("oauth2.googleapis.com/token") -> respond(
                            content = """{"access_token":"mock-token","expires_in":3600,"token_type":"Bearer"}""",
                            headers = jsonHeaders,
                        )
                        q.contains("name = 'EmuSync'") -> respond(
                            content = """{"files":[{"id":"root-id","name":"EmuSync"}]}""", headers = jsonHeaders,
                        )
                        q.contains("name = 'Legacy system'") -> respond(
                            content = """{"files":[{"id":"system-id","name":"Legacy system"}]}""", headers = jsonHeaders,
                        )
                        q.contains("name = 'Game'") -> respond(
                            content = """{"files":[{"id":"game-id","name":"Game"}]}""", headers = jsonHeaders,
                        )
                        request.url.parameters["alt"] == "media" -> respond(
                            "LEGACY_CLOUD_SAVE".toByteArray(),
                            headers = headersOf(HttpHeaders.ContentType, "application/octet-stream"),
                        )
                        q.contains("'game-id' in parents") -> respond(
                            content = """{"files":[{"id":"legacy-save-id","name":"SAVE_SLOT","modifiedTime":"$cloudModifiedTime"}]}""",
                            headers = jsonHeaders,
                        )
                        url.contains("upload/drive/v3/files") && request.method.value in setOf("POST", "PATCH") -> {
                            if (request.method.value == "PATCH") {
                                uploadRequests += "PATCH:${request.url.segments.last()}"
                            } else {
                                val metadata = Json.parseToJsonElement(
                                    (request.body as OutgoingContent.ByteArrayContent).bytes().decodeToString(),
                                ).jsonObject
                                currentUploadName = metadata["name"]?.jsonPrimitive?.content ?: ""
                                uploadRequests += "POST:$currentUploadName"
                            }
                            respond(
                                content = "",
                                headers = headersOf(HttpHeaders.Location, "https://uploads.example/legacy-file"),
                            )
                        }
                        request.method.value == "PUT" -> respond(
                            content = """{"id":"legacy-save-id","name":"$currentUploadName","modifiedTime":"2026-10-09T15:00:00.000Z"}""",
                            headers = jsonHeaders,
                        )
                        else -> respond(content = """{"files":[]}""", headers = jsonHeaders)
                    }
                }
            }
        }

        val orchestrator = SyncOrchestrator(
            client = mockClient,
            config = migratedConfig,
            configManager = configManager,
            onStatus = { status ->
                if (status == AppStatus.Playing) {
                    sawRestoredFileBeforePlay = legacySave.isFile && legacySave.readText() == "LEGACY_CLOUD_SAVE"
                    if (sawRestoredFileBeforePlay) {
                        legacySave.writeText("UPDATED_DURING_PLAY")
                        legacySave.setLastModified(Instant.now().plusSeconds(30).toEpochMilli())
                    }
                }
            },
        )

        orchestrator.playWithSync(
            GameItem(name = romFile.nameWithoutExtension, entry = entry, romFile = romFile),
        )

        assertTrue(sawRestoredFileBeforePlay, "The exact cloud match should be restored as a file before launch")
        assertEquals("UPDATED_DURING_PLAY", legacySave.readText())
        assertEquals(setOf("PATCH:legacy-save-id"), uploadRequests.toSet())
    }

    private fun createMockDriveClient(
        cloudFiles: List<Pair<String, String>>, // Pair(fileName, modifiedTimeIso)
        fileContents: Map<String, ByteArray> = emptyMap(),
        uploadResponseModTime: String = "2026-06-01T15:00:00.000Z",
        onWrite: () -> Unit = {},
        onUpdate: (String) -> Unit = {},
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
                        // Start a resumable upload or update session
                        url.contains("upload/drive/v3/files") && request.method.value in setOf("POST", "PATCH") -> {
                            if (request.method.value == "PATCH") {
                                onUpdate(request.url.segments.last())
                            }
                            onWrite()
                            respond(
                                content = "",
                                headers = headersOf(HttpHeaders.Location, "https://uploads.example/directory-sync"),
                            )
                        }
                        // Complete the media upload
                        request.method.value == "PUT" -> {
                            onWrite()
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
    fun `manual upload updates the cloud duplicate selected by the planner`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)
        val localSaveDir = File(tempDir, "savedata").apply { mkdirs() }
        File(localSaveDir, "save.dat").apply {
            writeText("LOCAL_SAVE")
            setLastModified(Instant.parse("2026-03-01T12:00:00Z").toEpochMilli())
        }
        val entry = NativePCGame(
            name = "Playstation 3",
            executablePath = "rpcs3",
            saveTargets = listOf(SaveTarget(localSaveDir.absolutePath, SaveTargetType.DIRECTORY)),
        )
        val config = driveConfig().copy(entries = listOf(entry))
        configManager.save(config)

        val updatedCloudIds = mutableListOf<String>()
        val statuses = mutableListOf<AppStatus>()
        val mockClient = createMockDriveClient(
            cloudFiles = listOf(
                "save.dat" to "2026-01-01T12:00:00.000Z",
                "save.dat" to "2026-02-01T12:00:00.000Z",
            ),
            onUpdate = updatedCloudIds::add,
        )
        val orchestrator = SyncOrchestrator(
            client = mockClient,
            config = config,
            configManager = configManager,
            onStatus = { status ->
                statuses += status
                if (status is AppStatus.Conflict) status.onResolve(SyncDecision.UPLOAD_LOCAL)
            },
        )

        assertTrue(orchestrator.syncEntry(entry))
        assertTrue(statuses.any { it is AppStatus.Conflict })
        assertEquals(listOf("file-1"), updatedCloudIds)
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
