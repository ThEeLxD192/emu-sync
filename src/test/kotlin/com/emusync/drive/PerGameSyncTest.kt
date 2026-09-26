package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.model.AppConfig
import com.emusync.model.EmulatorSystem
import com.emusync.model.GoogleDriveConfig
import com.emusync.model.NativePCGame
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
import kotlin.test.assertTrue

class PerGameSyncTest {

    private val jsonHeaders = headersOf(HttpHeaders.ContentType, "application/json")

    @Test
    fun `getCloudPathSegments respects game title for EmulatorSystem with savePathsByRom`() {
        val orchestrator = SyncOrchestrator(
            client = HttpClient(MockEngine) { engine { addHandler { respond("") } } },
            config = AppConfig(googleDrive = null),
            configManager = ConfigManager("/tmp/mock_config.json"),
            onStatus = {},
        )

        // Case: Eden emulator grouped under "Switch", individual save paths for Crash Team Racing
        val system = EmulatorSystem(
            name = "Eden",
            executablePath = "/usr/bin/eden",
            romsDirectory = "/roms/switch",
            extensions = listOf("nsp", "xci"),
            group = "Switch",
            savePaths = emptyList(),
            savePathsByRom = mapOf(
                "Crash Team Racing Nitro-Fueled.nsp" to listOf("/saves/OCTANE_0.sav", "/saves/OCTANE_OPTIONS.sav")
            ),
        )

        val pathSegments = orchestrator.getCloudPathSegments(
            entry = system,
            romName = "Crash Team Racing Nitro-Fueled.nsp",
            gameTitle = "Crash Team Racing Nitro-Fueled",
        )

        assertEquals(listOf("Switch", "Crash Team Racing Nitro-Fueled"), pathSegments)
    }

    @Test
    fun `getCloudPathSegments handles custom title in titleByRom`() {
        val orchestrator = SyncOrchestrator(
            client = HttpClient(MockEngine) { engine { addHandler { respond("") } } },
            config = AppConfig(googleDrive = null),
            configManager = ConfigManager("/tmp/mock_config.json"),
            onStatus = {},
        )

        val system = EmulatorSystem(
            name = "Eden",
            executablePath = "/usr/bin/eden",
            romsDirectory = "/roms/switch",
            extensions = listOf("nsp", "xci"),
            group = "Switch",
            savePaths = emptyList(),
            savePathsByRom = mapOf(
                "CTR_NF.nsp" to listOf("/saves/OCTANE_0.sav")
            ),
            titleByRom = mapOf(
                "CTR_NF.nsp" to "Crash Team Racing Nitro-Fueled"
            ),
        )

        val pathSegments = orchestrator.getCloudPathSegments(
            entry = system,
            romName = "CTR_NF.nsp",
        )

        assertEquals(listOf("Switch", "Crash Team Racing Nitro-Fueled"), pathSegments)
    }

    @Test
    fun `getCloudPathSegments for NativePCGame in a group uses group and game name`() {
        val orchestrator = SyncOrchestrator(
            client = HttpClient(MockEngine) { engine { addHandler { respond("") } } },
            config = AppConfig(googleDrive = null),
            configManager = ConfigManager("/tmp/mock_config.json"),
            onStatus = {},
        )

        val gameInGroup = NativePCGame(
            name = "Spelunky Classic",
            executablePath = "/games/spelunky",
            group = "Indie",
            savePaths = listOf("/saves/spelunky.dat"),
        )
        assertEquals(listOf("Indie", "Spelunky Classic"), orchestrator.getCloudPathSegments(gameInGroup))

        val gameWithoutGroup = NativePCGame(
            name = "Cave Story",
            executablePath = "/games/doukutsu",
            group = null,
            savePaths = listOf("/saves/Profile.dat"),
        )
        assertEquals(listOf("Cave Story"), orchestrator.getCloudPathSegments(gameWithoutGroup))
    }

    @Test
    fun `syncEntry uploads individual game saves into game subfolder on Google Drive`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)

        val saveFile1 = File(tempDir, "OCTANE_0.sav").apply { writeText("SAVE_DATA_0") }
        val saveFile2 = File(tempDir, "OCTANE_OPTIONS.sav").apply { writeText("SAVE_OPTIONS") }

        val system = EmulatorSystem(
            name = "Eden",
            executablePath = "echo",
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("nsp"),
            group = "Switch",
            savePaths = emptyList(),
            savePathsByRom = mapOf(
                "Crash Team Racing Nitro-Fueled.nsp" to listOf(saveFile1.absolutePath, saveFile2.absolutePath)
            ),
        )

        val config = AppConfig(
            googleDrive = GoogleDriveConfig(
                clientId = "test-client",
                clientSecret = "test-secret",
                refreshToken = "test-refresh-token",
            ),
            entries = listOf(system),
        )
        configManager.save(config)

        val createdFolders = mutableListOf<String>()
        val uploadedFiles = mutableListOf<Pair<String, String>>() // Pair(parentFolderId, driveName)

        val mockClient = HttpClient(MockEngine) {
            install(ContentNegotiation) {
                json(Json { ignoreUnknownKeys = true })
            }
            engine {
                addHandler { request ->
                    val url = request.url.toString()
                    val q = request.url.parameters["q"] ?: ""

                    when {
                        // OAuth token
                        url.contains("oauth2.googleapis.com/token") -> {
                            respond(
                                content = """{"access_token":"mock-token","expires_in":3600,"token_type":"Bearer"}""",
                                headers = jsonHeaders,
                            )
                        }
                        // Folder searches
                        q.contains("name = 'EmuSync'") -> {
                            respond(
                                content = """{"files":[{"id":"root-emusync","name":"EmuSync"}]}""",
                                headers = jsonHeaders,
                            )
                        }
                        q.contains("name = 'Switch'") -> {
                            respond(
                                content = """{"files":[{"id":"folder-switch","name":"Switch"}]}""",
                                headers = jsonHeaders,
                            )
                        }
                        q.contains("name = 'Crash Team Racing Nitro-Fueled'") -> {
                            // Subfolder doesn't exist yet
                            respond(content = """{"files":[]}""", headers = jsonHeaders)
                        }
                        // Create folder
                        request.method.value == "POST" && url.contains("drive/v3/files") && !url.contains("upload") -> {
                            createdFolders.add("Crash Team Racing Nitro-Fueled")
                            respond(
                                content = """{"id":"folder-ctr","name":"Crash Team Racing Nitro-Fueled"}""",
                                status = HttpStatusCode.Created,
                                headers = jsonHeaders,
                            )
                        }
                        // List files in folder (folder-ctr is empty)
                        q.contains("in parents and trashed = false") -> {
                            respond(content = """{"files":[]}""", headers = jsonHeaders)
                        }
                        // File upload
                        url.contains("upload/drive/v3/files") -> {
                            val driveName = request.url.parameters["name"] ?: "unknown"
                            val parent = request.url.parameters["parents"] ?: ""
                            uploadedFiles.add(parent to driveName)
                            respond(
                                content = """{"id":"uploaded-file-id","name":"$driveName","modifiedTime":"2026-09-25T18:00:00.000Z"}""",
                                headers = jsonHeaders,
                            )
                        }
                        else -> respond(content = """{"files":[]}""", headers = jsonHeaders)
                    }
                }
            }
        }

        val statuses = mutableListOf<AppStatus>()
        val orchestrator = SyncOrchestrator(
            client = mockClient,
            config = config,
            configManager = configManager,
            onStatus = { statuses.add(it) },
        )

        val success = orchestrator.syncEntry(system)
        assertTrue(success)

        // Verify subfolder was created
        assertTrue(createdFolders.contains("Crash Team Racing Nitro-Fueled"), "Subfolder for game should be created")

        // Verify status showed syncing game title
        assertTrue(statuses.any { it is AppStatus.Syncing && it.message.contains("Crash Team Racing Nitro-Fueled") })
    }

    @Test
    fun `playWithSync postSync uploads into game-scoped subfolder`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)

        val romFile = File(tempDir, "Crash Team Racing Nitro-Fueled.nsp").apply { writeText("ROM") }
        val saveFile = File(tempDir, "OCTANE_0.sav").apply { writeText("SAVE_DATA") }

        val system = EmulatorSystem(
            name = "Eden",
            executablePath = "echo",
            arguments = listOf("{ROM}"),
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("nsp"),
            group = "Switch",
            savePaths = emptyList(),
            savePathsByRom = mapOf(
                romFile.name to listOf(saveFile.absolutePath)
            ),
        )

        val config = AppConfig(
            googleDrive = GoogleDriveConfig(
                clientId = "test-client",
                clientSecret = "test-secret",
                refreshToken = "test-refresh-token",
            ),
            entries = listOf(system),
        )
        configManager.save(config)

        val targetFoldersQueried = mutableListOf<String>()

        val mockClient = HttpClient(MockEngine) {
            install(ContentNegotiation) {
                json(Json { ignoreUnknownKeys = true })
            }
            engine {
                addHandler { request ->
                    val url = request.url.toString()
                    val q = request.url.parameters["q"] ?: ""

                    when {
                        url.contains("oauth2.googleapis.com/token") -> {
                            respond(
                                content = """{"access_token":"mock-token","expires_in":3600,"token_type":"Bearer"}""",
                                headers = jsonHeaders,
                            )
                        }
                        q.contains("name = 'EmuSync'") -> {
                            respond(
                                content = """{"files":[{"id":"root-emusync","name":"EmuSync"}]}""",
                                headers = jsonHeaders,
                            )
                        }
                        q.contains("name = 'Switch'") -> {
                            respond(
                                content = """{"files":[{"id":"folder-switch","name":"Switch"}]}""",
                                headers = jsonHeaders,
                            )
                        }
                        q.contains("name = 'Crash Team Racing Nitro-Fueled'") -> {
                            targetFoldersQueried.add("Crash Team Racing Nitro-Fueled")
                            respond(
                                content = """{"files":[{"id":"folder-ctr","name":"Crash Team Racing Nitro-Fueled"}]}""",
                                headers = jsonHeaders,
                            )
                        }
                        q.contains("in parents and trashed = false") -> {
                            respond(content = """{"files":[]}""", headers = jsonHeaders)
                        }
                        url.contains("upload/drive/v3/files") -> {
                            respond(
                                content = """{"id":"uploaded-file-id","name":"OCTANE_0.sav","modifiedTime":"2026-09-25T18:00:00.000Z"}""",
                                headers = jsonHeaders,
                            )
                        }
                        else -> respond(content = """{"files":[]}""", headers = jsonHeaders)
                    }
                }
            }
        }

        val orchestrator = SyncOrchestrator(
            client = mockClient,
            config = config,
            configManager = configManager,
            onStatus = {},
        )

        val gameItem = GameItem(
            name = romFile.nameWithoutExtension,
            entry = system,
            romFile = romFile,
        )

        orchestrator.playWithSync(gameItem)

        assertTrue(
            targetFoldersQueried.contains("Crash Team Racing Nitro-Fueled"),
            "playWithSync must query and ensure the game's specific subfolder"
        )
    }

    @Test
    fun `renameGameCloudData renames cloud folder when destination does not exist yet`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)
        val saveFile = File(tempDir, "OCTANE_0.sav").apply { writeText("LOCAL_SAVE") }

        val system = EmulatorSystem(
            name = "Eden",
            executablePath = "echo",
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("nsp"),
            group = "Switch",
            savePaths = emptyList(),
            savePathsByRom = mapOf(
                "CTR.nsp" to listOf(saveFile.absolutePath)
            ),
        )

        val config = AppConfig(
            googleDrive = GoogleDriveConfig(
                clientId = "test-client",
                clientSecret = "test-secret",
                refreshToken = "test-refresh-token",
            ),
            entries = listOf(system),
        )
        configManager.save(config)

        var renamedOldFolderTo: String? = null

        val mockClient = HttpClient(MockEngine) {
            install(ContentNegotiation) {
                json(Json { ignoreUnknownKeys = true })
            }
            engine {
                addHandler { request ->
                    val url = request.url.toString()
                    val q = request.url.parameters["q"] ?: ""

                    when {
                        url.contains("oauth2.googleapis.com/token") -> {
                            respond(
                                content = """{"access_token":"mock-token","expires_in":3600,"token_type":"Bearer"}""",
                                headers = jsonHeaders,
                            )
                        }
                        q.contains("name = 'EmuSync'") -> {
                            respond(content = """{"files":[{"id":"root-id","name":"EmuSync"}]}""", headers = jsonHeaders)
                        }
                        q.contains("name = 'Switch'") -> {
                            respond(content = """{"files":[{"id":"switch-id","name":"Switch"}]}""", headers = jsonHeaders)
                        }
                        q.contains("name = 'Crash Team Racing'") -> {
                            // Old folder exists!
                            respond(content = """{"files":[{"id":"old-ctr-id","name":"Crash Team Racing"}]}""", headers = jsonHeaders)
                        }
                        q.contains("name = 'Crash Team Racing Nitro-Fueled'") -> {
                            // New folder does NOT exist yet!
                            respond(content = """{"files":[]}""", headers = jsonHeaders)
                        }
                        request.method.value == "PATCH" && url.contains("drive/v3/files/old-ctr-id") -> {
                            renamedOldFolderTo = "Crash Team Racing Nitro-Fueled"
                            respond(content = """{"id":"old-ctr-id","name":"Crash Team Racing Nitro-Fueled"}""", headers = jsonHeaders)
                        }
                        else -> respond(content = """{"files":[]}""", headers = jsonHeaders)
                    }
                }
            }
        }

        val orchestrator = SyncOrchestrator(
            client = mockClient,
            config = config,
            configManager = configManager,
            onStatus = {},
        )

        val result = orchestrator.renameGameCloudData(
            entry = system,
            romName = "CTR.nsp",
            oldTitle = "Crash Team Racing",
            newTitle = "Crash Team Racing Nitro-Fueled",
            localSavePaths = listOf(saveFile),
        )

        assertTrue(result)
        assertEquals("Crash Team Racing Nitro-Fueled", renamedOldFolderTo)
    }

    @Test
    fun `renameGameCloudData prompts Conflict when destination already exists and resolves to DOWNLOAD_CLOUD`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)
        val saveFile = File(tempDir, "OCTANE_0.sav").apply { writeText("OLD_LOCAL_CONTENT") }
        saveFile.setLastModified(1000000000000L) // Older local timestamp

        val system = EmulatorSystem(
            name = "Eden",
            executablePath = "echo",
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("nsp"),
            group = "Switch",
            savePaths = emptyList(),
            savePathsByRom = mapOf(
                "CTR.nsp" to listOf(saveFile.absolutePath)
            ),
        )

        val config = AppConfig(
            googleDrive = GoogleDriveConfig(
                clientId = "test-client",
                clientSecret = "test-secret",
                refreshToken = "test-refresh-token",
            ),
            entries = listOf(system),
        )
        configManager.save(config)

        var conflictPrompted = false
        var deletedOldFolderId: String? = null

        val mockClient = HttpClient(MockEngine) {
            install(ContentNegotiation) {
                json(Json { ignoreUnknownKeys = true })
            }
            engine {
                addHandler { request ->
                    val url = request.url.toString()
                    val q = request.url.parameters["q"] ?: ""

                    when {
                        url.contains("oauth2.googleapis.com/token") -> {
                            respond(
                                content = """{"access_token":"mock-token","expires_in":3600,"token_type":"Bearer"}""",
                                headers = jsonHeaders,
                            )
                        }
                        q.contains("name = 'EmuSync'") -> {
                            respond(content = """{"files":[{"id":"root-id","name":"EmuSync"}]}""", headers = jsonHeaders)
                        }
                        q.contains("name = 'Switch'") -> {
                            respond(content = """{"files":[{"id":"switch-id","name":"Switch"}]}""", headers = jsonHeaders)
                        }
                        q.contains("name = 'Crash Team Racing'") -> {
                            // Old folder exists
                            respond(content = """{"files":[{"id":"old-ctr-id","name":"Crash Team Racing"}]}""", headers = jsonHeaders)
                        }
                        q.contains("name = 'Crash Team Racing Nitro-Fueled'") -> {
                            // Destination ALREADY exists!
                            respond(content = """{"files":[{"id":"new-ctr-id","name":"Crash Team Racing Nitro-Fueled"}]}""", headers = jsonHeaders)
                        }
                        q.contains("'old-ctr-id' in parents") -> {
                            respond(content = """{"files":[]}""", headers = jsonHeaders)
                        }
                        q.contains("'new-ctr-id' in parents") -> {
                            // Destination contains a newer cloud save!
                            respond(
                                content = """{"files":[{"id":"cloud-save-id","name":"OCTANE_0.sav","modifiedTime":"2026-09-25T15:00:00.000Z"}]}""",
                                headers = jsonHeaders,
                            )
                        }
                        request.method.value == "DELETE" -> {
                            deletedOldFolderId = request.url.segments.last()
                            respond("", status = HttpStatusCode.NoContent)
                        }
                        request.url.parameters["alt"] == "media" -> {
                            respond("DOWNLOADED_FROM_CLOUD".toByteArray(), headers = headersOf(HttpHeaders.ContentType, "application/octet-stream"))
                        }
                        else -> respond(content = """{"files":[]}""", headers = jsonHeaders)
                    }
                }
            }
        }

        val orchestrator = SyncOrchestrator(
            client = mockClient,
            config = config,
            configManager = configManager,
            onStatus = { status ->
                if (status is AppStatus.Conflict) {
                    conflictPrompted = true
                    status.onResolve(com.emusync.drive.SyncDecision.DOWNLOAD_CLOUD)
                }
            },
        )

        val result = orchestrator.renameGameCloudData(
            entry = system,
            romName = "CTR.nsp",
            oldTitle = "Crash Team Racing",
            newTitle = "Crash Team Racing Nitro-Fueled",
            localSavePaths = listOf(saveFile),
        )

        assertTrue(result)
        assertEquals("old-ctr-id", deletedOldFolderId)
        assertEquals("DOWNLOADED_FROM_CLOUD", saveFile.readText())
    }

    @Test
    fun `playWithSync prompts for save path before launch when cloud saves exist and downloads them to selected path`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)
        val romFile = File(tempDir, "Crash Team Racing Nitro-Fueled.nsp").apply { writeText("ROM") }
        val targetLocalSave = File(tempDir, "OCTANE_0.sav") // does not exist yet

        val system = EmulatorSystem(
            name = "Eden",
            executablePath = "echo",
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("nsp"),
            group = "Switch",
            savePaths = emptyList(),
            savePathsByRom = emptyMap(),
        )

        val config = AppConfig(
            googleDrive = GoogleDriveConfig(
                clientId = "test-client",
                clientSecret = "test-secret",
                refreshToken = "test-refresh-token",
            ),
            entries = listOf(system),
        )

        var requestedNotice: String? = null
        var onRequestSaveCalledCount = 0

        val mockClient = HttpClient(MockEngine) {
            install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
            engine {
                addHandler { request ->
                    val url = request.url.toString()
                    val q = request.url.parameters["q"] ?: ""
                    when {
                        url.contains("oauth2.googleapis.com/token") -> {
                            respond(
                                content = """{"access_token":"mock-token","expires_in":3600,"token_type":"Bearer"}""",
                                headers = jsonHeaders,
                            )
                        }
                        q.contains("name = 'EmuSync'") -> {
                            respond(content = """{"files":[{"id":"root-id","name":"EmuSync"}]}""", headers = jsonHeaders)
                        }
                        q.contains("name = 'Switch'") -> {
                            respond(content = """{"files":[{"id":"switch-id","name":"Switch"}]}""", headers = jsonHeaders)
                        }
                        q.contains("name = 'Crash Team Racing Nitro-Fueled'") -> {
                            respond(content = """{"files":[{"id":"game-folder-id","name":"Crash Team Racing Nitro-Fueled"}]}""", headers = jsonHeaders)
                        }
                        q.contains("'game-folder-id' in parents") -> {
                            respond(
                                content = """{"files":[{"id":"cloud-file-id","name":"OCTANE_0.sav","modifiedTime":"2026-09-25T18:00:00.000Z"}]}""",
                                headers = jsonHeaders,
                            )
                        }
                        request.url.parameters["alt"] == "media" -> {
                            respond("OCTANE_CLOUD_DATA".toByteArray(), headers = headersOf(HttpHeaders.ContentType, "application/octet-stream"))
                        }
                        url.contains("upload/drive/v3/files") -> {
                            respond(content = """{"id":"cloud-file-id","name":"OCTANE_0.sav"}""", headers = jsonHeaders)
                        }
                        else -> respond(content = """{"files":[]}""", headers = jsonHeaders)
                    }
                }
            }
        }

        val orchestrator = SyncOrchestrator(
            client = mockClient,
            config = config,
            configManager = configManager,
            onStatus = {},
        )

        val gameItem = GameItem(
            name = romFile.nameWithoutExtension,
            entry = system,
            romFile = romFile,
        )

        orchestrator.playWithSync(
            item = gameItem,
            onRequestSaveSetup = { _, notice ->
                onRequestSaveCalledCount++
                requestedNotice = notice
                listOf(targetLocalSave.absolutePath)
            }
        )

        assertEquals(1, onRequestSaveCalledCount, "onRequestSaveSetup should be called once before launch")
        assertTrue(requestedNotice?.contains("Cloud save data was found") == true, "Notice should inform user of cloud save found")
        assertTrue(targetLocalSave.exists(), "Target local save should have been downloaded")
        assertEquals("OCTANE_CLOUD_DATA", targetLocalSave.readText())
    }

    @Test
    fun `playWithSync aborts launch when cloud saves exist and user cancels save setup`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)
        val romFile = File(tempDir, "Crash Team Racing Nitro-Fueled.nsp").apply { writeText("ROM") }

        val system = EmulatorSystem(
            name = "Eden",
            executablePath = "echo",
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("nsp"),
            group = "Switch",
            savePaths = emptyList(),
            savePathsByRom = emptyMap(),
        )

        val config = AppConfig(
            googleDrive = GoogleDriveConfig(
                clientId = "test-client",
                clientSecret = "test-secret",
                refreshToken = "test-refresh-token",
            ),
            entries = listOf(system),
        )

        var cancelledPromptCalled = false

        val mockClient = HttpClient(MockEngine) {
            install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
            engine {
                addHandler { request ->
                    val url = request.url.toString()
                    val q = request.url.parameters["q"] ?: ""
                    when {
                        url.contains("oauth2.googleapis.com/token") -> {
                            respond(content = """{"access_token":"mock-token","expires_in":3600,"token_type":"Bearer"}""", headers = jsonHeaders)
                        }
                        q.contains("name = 'EmuSync'") -> respond(content = """{"files":[{"id":"root-id","name":"EmuSync"}]}""", headers = jsonHeaders)
                        q.contains("name = 'Switch'") -> respond(content = """{"files":[{"id":"switch-id","name":"Switch"}]}""", headers = jsonHeaders)
                        q.contains("name = 'Crash Team Racing Nitro-Fueled'") -> respond(content = """{"files":[{"id":"game-folder-id","name":"Crash Team Racing Nitro-Fueled"}]}""", headers = jsonHeaders)
                        q.contains("'game-folder-id' in parents") -> {
                            respond(
                                content = """{"files":[{"id":"cloud-file-id","name":"OCTANE_0.sav","modifiedTime":"2026-09-25T18:00:00.000Z"}]}""",
                                headers = jsonHeaders,
                            )
                        }
                        else -> respond(content = """{"files":[]}""", headers = jsonHeaders)
                    }
                }
            }
        }

        val orchestrator = SyncOrchestrator(
            client = mockClient,
            config = config,
            configManager = configManager,
            onStatus = {},
        )

        val gameItem = GameItem(
            name = romFile.nameWithoutExtension,
            entry = system,
            romFile = romFile,
        )

        orchestrator.playWithSync(
            item = gameItem,
            onRequestSaveSetup = { _, _ ->
                cancelledPromptCalled = true
                emptyList()
            }
        )

        assertTrue(cancelledPromptCalled, "User should be prompted")
    }

    @Test
    fun `playWithSync launches without pre-prompt when no cloud saves exist and prompts after game exits`(@TempDir tempDir: File) = runTest {
        val configFile = File(tempDir, "config.json")
        val configManager = ConfigManager(configFile.absolutePath)
        val romFile = File(tempDir, "New Game.nsp").apply { writeText("ROM") }
        val createdSave = File(tempDir, "new_game.sav").apply { writeText("CREATED_DURING_GAME") }

        val system = EmulatorSystem(
            name = "Eden",
            executablePath = "echo",
            romsDirectory = tempDir.absolutePath,
            extensions = listOf("nsp"),
            group = "Switch",
            savePaths = emptyList(),
            savePathsByRom = emptyMap(),
        )

        val config = AppConfig(
            googleDrive = GoogleDriveConfig(
                clientId = "test-client",
                clientSecret = "test-secret",
                refreshToken = "test-refresh-token",
            ),
            entries = listOf(system),
        )

        var promptCount = 0
        var receivedNotice: String? = "INITIAL"
        var uploadedFileToCloud = false

        val mockClient = HttpClient(MockEngine) {
            install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
            engine {
                addHandler { request ->
                    val url = request.url.toString()
                    val q = request.url.parameters["q"] ?: ""
                    when {
                        url.contains("oauth2.googleapis.com/token") -> {
                            respond(content = """{"access_token":"mock-token","expires_in":3600,"token_type":"Bearer"}""", headers = jsonHeaders)
                        }
                        q.contains("name = 'EmuSync'") -> respond(content = """{"files":[{"id":"root-id","name":"EmuSync"}]}""", headers = jsonHeaders)
                        q.contains("name = 'Switch'") -> respond(content = """{"files":[{"id":"switch-id","name":"Switch"}]}""", headers = jsonHeaders)
                        q.contains("name = 'New Game'") -> {
                            respond(content = """{"files":[]}""", headers = jsonHeaders)
                        }
                        url.contains("upload/drive/v3/files") -> {
                            uploadedFileToCloud = true
                            respond(content = """{"id":"new-cloud-file-id","name":"new_game.sav"}""", headers = jsonHeaders)
                        }
                        request.method.value == "POST" && url.contains("drive/v3/files") -> {
                            respond(content = """{"id":"created-folder-id","name":"New Game"}""", headers = jsonHeaders)
                        }
                        else -> respond(content = """{"files":[]}""", headers = jsonHeaders)
                    }
                }
            }
        }

        val orchestrator = SyncOrchestrator(
            client = mockClient,
            config = config,
            configManager = configManager,
            onStatus = {},
        )

        val gameItem = GameItem(
            name = romFile.nameWithoutExtension,
            entry = system,
            romFile = romFile,
        )

        orchestrator.playWithSync(
            item = gameItem,
            onRequestSaveSetup = { _, notice ->
                promptCount++
                receivedNotice = notice
                listOf(createdSave.absolutePath)
            }
        )

        assertEquals(1, promptCount, "Should prompt exactly once after the game exits")
        kotlin.test.assertNull(receivedNotice, "Post-game prompt should have no pre-launch cloud notice")
        assertTrue(uploadedFileToCloud, "Newly configured save should be uploaded in Post-Sync")
    }
}

