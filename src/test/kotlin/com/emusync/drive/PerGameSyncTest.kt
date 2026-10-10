package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.model.AppConfig
import com.emusync.model.EmulatorSystem
import com.emusync.model.GoogleDriveConfig
import com.emusync.model.NativePCGame
import com.emusync.model.SaveTarget
import com.emusync.model.SaveTargetType
import com.emusync.model.inferLegacySaveTarget
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
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import java.time.Instant
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class PerGameSyncTest {

    private val jsonHeaders = headersOf(HttpHeaders.ContentType, "application/json")

    @Test
    fun `unknown missing target ignores exact cloud name owned by sibling target`(@TempDir tempDir: File) {
        val missingTarget = File(tempDir, "SAVE_SLOT")
        val cloudSave = DriveFile("save-id", "SAVE_SLOT", "2026-09-25T18:00:00.000Z")

        assertFalse(
            isDirectorySaveTarget(SaveTargetType.UNKNOWN, missingTarget, listOf(cloudSave)),
            "A matching Drive filename identifies a missing legacy target as a file",
        )
        assertTrue(
            isDirectorySaveTarget(SaveTargetType.UNKNOWN, missingTarget, listOf(cloudSave), setOf("SAVE_SLOT")),
            "A matching cloud file claimed by a known file target must not classify this target as a file",
        )
        assertFalse(
            isDirectorySaveTarget(SaveTargetType.UNKNOWN, missingTarget, listOf(cloudSave)),
            "An UNKNOWN sibling with the same name cannot claim the exact cloud match",
        )
        assertTrue(
            isDirectorySaveTarget(SaveTargetType.UNKNOWN, missingTarget, listOf(cloudSave.copy(name = "slot/save.dat"))),
            "Nested cloud names identify a missing legacy target as a directory",
        )
    }

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
    fun `global save paths use emulator cloud folder despite empty or copied rom overrides`() {
        val orchestrator = SyncOrchestrator(
            client = HttpClient(MockEngine) { engine { addHandler { respond("") } } },
            config = AppConfig(googleDrive = null),
            configManager = ConfigManager("/tmp/mock_config.json"),
            onStatus = {},
        )
        val baseSystem = EmulatorSystem(
            name = "RPCS3",
            executablePath = "/usr/bin/rpcs3",
            romsDirectory = "/games/ps3",
            extensions = listOf("iso"),
            group = "PlayStation",
            savePaths = listOf("/mnt/Juegos/Saves/PS3"),
        )

        val emptyOverride = baseSystem.copy(
            savePathsByRom = mapOf("Black.iso" to emptyList()),
        )
        val copiedGlobalOverride = baseSystem.copy(
            savePathsByRom = mapOf("Black.iso" to listOf("/mnt/Juegos/Saves/PS3")),
        )
        val distinctPerGameOverride = baseSystem.copy(
            savePathsByRom = mapOf("Black.iso" to listOf("/mnt/Juegos/Saves/PS3/Black")),
        )

        assertEquals(
            listOf("PlayStation"),
            orchestrator.getCloudPathSegments(baseSystem, "Black.iso", "Black"),
        )
        assertEquals(
            listOf("PlayStation"),
            orchestrator.getCloudPathSegments(emptyOverride, "Black.iso", "Black"),
        )
        assertEquals(
            listOf("PlayStation"),
            orchestrator.getCloudPathSegments(copiedGlobalOverride, "Black.iso", "Black"),
        )
        assertEquals(
            listOf("PlayStation", "Black"),
            orchestrator.getCloudPathSegments(distinctPerGameOverride, "Black.iso", "Black"),
        )
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
        var currentUploadName = "mock-file"

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
                        // Start a resumable upload or update session
                        url.contains("upload/drive/v3/files") && request.method.value in setOf("POST", "PATCH") -> {
                            if (request.method.value == "POST") {
                                val metadata = Json.parseToJsonElement(
                                    (request.body as OutgoingContent.ByteArrayContent).bytes().decodeToString()
                                ).jsonObject
                                val driveName = metadata["name"]?.jsonPrimitive?.content ?: "unknown"
                                currentUploadName = driveName
                                val parent = metadata["parents"]?.jsonArray?.single()?.jsonPrimitive?.content ?: ""
                                uploadedFiles.add(parent to driveName)
                            }
                            respond(
                                content = "",
                                headers = headersOf(HttpHeaders.Location, "https://uploads.example/per-game-create"),
                            )
                        }
                        request.method.value == "PUT" -> {
                            respond(
                                content = """{"id":"uploaded-file-id","name":"$currentUploadName","modifiedTime":"2026-09-25T18:00:00.000Z"}""",
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
        assertTrue(success, "Sync failed with statuses: $statuses")

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
                        url.contains("upload/drive/v3/files") && request.method.value in setOf("POST", "PATCH") -> {
                            respond(
                                content = "",
                                headers = headersOf(HttpHeaders.Location, "https://uploads.example/per-game-post"),
                            )
                        }
                        request.method.value == "PUT" -> {
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
            localSaveTargets = listOf(SaveTarget(saveFile.absolutePath, SaveTargetType.FILE)),
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
            localSaveTargets = listOf(SaveTarget(saveFile.absolutePath, SaveTargetType.FILE)),
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
        val destinationFolder = File(tempDir, "shared-ps1-saves").apply { mkdirs() }
        val unrelatedLocalSave = File(destinationFolder, "Other Game.mcd").apply { writeText("OTHER_GAME_DATA") }
        val cloudModifiedTime = Instant.now().minusSeconds(120).toString()

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
        var requestedCloudFileNames: List<String> = emptyList()
        var onRequestSaveCalledCount = 0
        var uploadSessionCount = 0

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
                                content = """{"files":[{"id":"cloud-file-id-0","name":"Bugs Bunny_1.mcd","modifiedTime":"$cloudModifiedTime"},{"id":"cloud-file-id-1","name":"Bugs Bunny_2.mcd","modifiedTime":"$cloudModifiedTime"}]}""",
                                headers = jsonHeaders,
                            )
                        }
                        request.url.parameters["alt"] == "media" -> {
                            respond("OCTANE_CLOUD_DATA".toByteArray(), headers = headersOf(HttpHeaders.ContentType, "application/octet-stream"))
                        }
                        url.contains("upload/drive/v3/files") && request.method.value in setOf("POST", "PATCH") -> {
                            uploadSessionCount++
                            respond(content = "", headers = headersOf(HttpHeaders.Location, "https://uploads.example/per-game-setup"))
                        }
                        request.method.value == "PUT" -> {
                            respond(
                                content = """{"id":"uploaded-file","name":"save.mcd","modifiedTime":"$cloudModifiedTime"}""",
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
            onRequestSaveSetup = { _, notice, cloudFileNames ->
                onRequestSaveCalledCount++
                requestedNotice = notice
                requestedCloudFileNames = cloudFileNames
                resolveCloudRestorePaths(destinationFolder.absolutePath, cloudFileNames)
                    .map { SaveTarget(it, SaveTargetType.FILE) }
            }
        )

        assertEquals(1, onRequestSaveCalledCount, "onRequestSaveSetup should be called once before launch")
        assertTrue(requestedNotice?.contains("Cloud save data was found") == true, "Notice should inform user of cloud save found")
        assertEquals(listOf("Bugs Bunny_1.mcd", "Bugs Bunny_2.mcd"), requestedCloudFileNames)
        assertEquals("OCTANE_CLOUD_DATA", File(destinationFolder, "Bugs Bunny_1.mcd").readText())
        assertEquals("OCTANE_CLOUD_DATA", File(destinationFolder, "Bugs Bunny_2.mcd").readText())
        assertEquals("OTHER_GAME_DATA", unrelatedLocalSave.readText())
        assertEquals(0, uploadSessionCount, "Unrelated files in the chosen folder must not be uploaded")
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
            onRequestSaveSetup = { _, _, _ ->
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
                        url.contains("upload/drive/v3/files") && request.method.value in setOf("POST", "PATCH") -> {
                            uploadedFileToCloud = true
                            respond(content = "", headers = headersOf(HttpHeaders.Location, "https://uploads.example/new-save"))
                        }
                        request.method.value == "PUT" -> {
                            respond(
                                content = """{"id":"new-cloud-file-id","name":"new_game.sav","modifiedTime":"2026-09-25T18:00:00.000Z"}""",
                                headers = jsonHeaders,
                            )
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
            onRequestSaveSetup = { _, notice, _ ->
                promptCount++
                receivedNotice = notice
                listOf(SaveTarget(createdSave.absolutePath, SaveTargetType.FILE))
            }
        )

        assertEquals(1, promptCount, "Should prompt exactly once after the game exits")
        kotlin.test.assertNull(receivedNotice, "Post-game prompt should have no pre-launch cloud notice")
        assertTrue(uploadedFileToCloud, "Newly configured save should be uploaded in Post-Sync")
    }
}

