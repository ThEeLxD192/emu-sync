package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.model.AppConfig
import com.emusync.model.EmulatorSystem
import com.emusync.model.GameEntry
import com.emusync.model.GoogleDriveConfig
import com.emusync.model.NativePCGame
import com.emusync.ui.AppStatus
import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.content.OutgoingContent
import io.ktor.http.headersOf
import io.ktor.serialization.kotlinx.json.json
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class CloudFolderMigrationTest {
    @Test
    fun `files added by another device during merge remain in the source`() = runTest {
        MigrationDrive().use { drive ->
            drive.folder("source", "Old")
            drive.folder("destination", "New")
            drive.file("save", "save.dat", "source")
            drive.onMove = { drive.file("new-arrival", "offline-progress.dat", "source") }

            migrator(drive).migrate("token", listOf("Old"), listOf("New"))

            assertEquals("destination", drive.nodes.getValue("save").parent)
            assertEquals("source", drive.nodes.getValue("new-arrival").parent)
            assertTrue("source" in drive.nodes)
            assertTrue(drive.deletions.isEmpty())
        }
    }

    @Test
    fun `successful merge moves files and nested folders while keeping the source`(@TempDir directory: File) = runTest {
        MigrationDrive().use { drive ->
            drive.folder("source", "Old")
            drive.folder("destination", "New")
            drive.file("save", "save.dat", "source")
            drive.folder("nested", "Nested", "source")
            drive.file("nested-save", "slot.dat", "nested")
            drive.file("existing", "other.dat", "destination")

            val old = NativePCGame("Old", "echo")
            assertTrue(orchestrator(drive, directory, old).renameEntryCloudData(old, old.copy(name = "New")))

            assertEquals("destination", drive.nodes.getValue("save").parent)
            assertEquals("destination", drive.nodes.getValue("nested").parent)
            assertEquals("nested", drive.nodes.getValue("nested-save").parent)
            assertTrue(drive.children("source").isEmpty())
            assertTrue("source" in drive.nodes)
            assertTrue(drive.deletions.isEmpty())
        }
    }

    @Test
    fun `game rename refuses duplicate names before moving unique saves`(@TempDir directory: File) = runTest {
        MigrationDrive().use { drive ->
            drive.folder("console", "Console")
            drive.folder("source", "Old", "console")
            drive.folder("destination", "New", "console")
            drive.file("unique", "unique.dat", "source")
            drive.file("old-save", "save.dat", "source", "OLD PROGRESS")
            drive.file("new-save", "save.dat", "destination", "OTHER PROGRESS")
            val entry = EmulatorSystem("Console", "echo", romsDirectory = directory.path, extensions = listOf("rom"))
            val statuses = mutableListOf<AppStatus>()

            val success = orchestrator(drive, directory, entry, statuses).renameGameCloudData(
                entry, "Game.rom", "Old", "New", emptyList(),
            )

            assertFalse(success)
            assertTrue(drive.moves.isEmpty())
            assertTrue(drive.deletions.isEmpty())
            assertEquals("OLD PROGRESS", drive.nodes.getValue("old-save").content)
            assertEquals("OTHER PROGRESS", drive.nodes.getValue("new-save").content)
            assertTrue((statuses.last() as AppStatus.Error).message.contains("save.dat"))
        }
    }

    @Test
    fun `matching subfolders are preserved instead of creating ambiguous duplicate folders`() = runTest {
        MigrationDrive().use { drive ->
            drive.folder("source", "Old")
            drive.folder("destination", "New")
            drive.folder("old-game", "Game", "source")
            drive.folder("new-game", "Game", "destination")

            assertFailsWith<DriveApiException> {
                migrator(drive).migrate("token", listOf("Old"), listOf("New"))
            }

            assertTrue(drive.moves.isEmpty())
            assertEquals("source", drive.nodes.getValue("old-game").parent)
            assertEquals("destination", drive.nodes.getValue("new-game").parent)
            assertTrue(drive.deletions.isEmpty())
        }
    }

    @Test
    fun `duplicate names already in source also block a merge`() = runTest {
        MigrationDrive().use { drive ->
            drive.folder("source", "Old")
            drive.folder("destination", "New")
            drive.file("first", "save.dat", "source")
            drive.file("second", "save.dat", "source")

            assertFailsWith<DriveApiException> {
                migrator(drive).migrate("token", listOf("Old"), listOf("New"))
            }
            assertTrue(drive.moves.isEmpty())
        }
    }

    @Test
    fun `failed move restores previous moves and reports failure`(@TempDir directory: File) = runTest {
        MigrationDrive().use { drive ->
            drive.folder("source", "Old")
            drive.folder("destination", "New")
            drive.file("first", "first.dat", "source")
            drive.file("second", "second.dat", "source")
            drive.failMove = { it.id == "second" && it.to == "destination" }
            val entry = NativePCGame("Old", "echo")
            val statuses = mutableListOf<AppStatus>()

            assertFalse(orchestrator(drive, directory, entry, statuses).renameEntryCloudData(entry, entry.copy(name = "New")))

            assertEquals(setOf("first", "second"), drive.children("source").map { it.id }.toSet())
            assertTrue(drive.children("destination").isEmpty())
            assertEquals(MigrationDrive.Move("first", "destination", "source"), drive.moves.last())
            assertTrue(statuses.last() is AppStatus.Error)
            assertTrue(drive.deletions.isEmpty())

            // A retry after the transient failure must move the same IDs without duplication.
            drive.failMove = { false }
            assertTrue(orchestrator(drive, directory, entry).renameEntryCloudData(entry, entry.copy(name = "New")))
            assertEquals(setOf("first", "second"), drive.children("destination").map { it.id }.toSet())
        }
    }

    @Test
    fun `failed rollback retains files on both sides and identifies recovery location`() = runTest {
        MigrationDrive().use { drive ->
            drive.folder("source", "Old")
            drive.folder("destination", "New")
            drive.file("first", "first.dat", "source")
            drive.file("second", "second.dat", "source")
            drive.failMove = { it.id == "second" || it.to == "source" }

            val failure = assertFailsWith<DriveApiException> {
                migrator(drive).migrate("token", listOf("Old"), listOf("New"))
            }

            assertEquals("destination", drive.nodes.getValue("first").parent)
            assertEquals("source", drive.nodes.getValue("second").parent)
            assertTrue(failure.message!!.contains("New"))
            assertTrue(failure.message!!.contains("first.dat"))
            assertTrue(drive.deletions.isEmpty())
        }
    }

    @Test
    fun `cancellation restores acknowledged moves and propagates`(@TempDir directory: File) = runTest {
        MigrationDrive().use { drive ->
            drive.folder("source", "Old")
            drive.folder("destination", "New")
            drive.file("first", "first.dat", "source")
            drive.file("second", "second.dat", "source")
            drive.onMove = { if (it.id == "second") throw CancellationException("Cancelled migration") }
            val entry = NativePCGame("Old", "echo")
            val statuses = mutableListOf<AppStatus>()

            assertFailsWith<CancellationException> {
                orchestrator(drive, directory, entry, statuses).renameEntryCloudData(entry, entry.copy(name = "New"))
            }

            assertEquals(setOf("first", "second"), drive.children("source").map { it.id }.toSet())
            assertFalse(statuses.any { it is AppStatus.Error })
            assertTrue(drive.deletions.isEmpty())
        }
    }

    @Test
    fun `native group change moves only its game folder`(@TempDir directory: File) = runTest {
        MigrationDrive().use { drive ->
            drive.folder("old-group", "Old Group")
            drive.folder("new-group", "New Group")
            drive.folder("game", "Game", "old-group")
            drive.folder("other", "Other", "old-group")
            drive.file("save", "save.dat", "game")
            val entry = NativePCGame("Game", "echo", group = "Old Group")
            val other = NativePCGame("Other", "echo", group = "Old Group")

            assertTrue(orchestrator(drive, directory, entry, otherEntries = listOf(other))
                .renameEntryCloudData(entry, entry.copy(group = "New Group")))

            assertEquals("new-group", drive.nodes.getValue("game").parent)
            assertEquals("old-group", drive.nodes.getValue("other").parent)
            assertEquals("game", drive.nodes.getValue("save").parent)
            assertTrue(drive.deletions.isEmpty())
        }
    }

    @Test
    fun `paths resolving to the same folder do not move or delete its contents`() = runTest {
        MigrationDrive().use { drive ->
            drive.folder("source", "Old")
            drive.file("save", "save.dat", "source")

            migrator(drive).migrate("token", listOf("Old "), listOf("Old"))

            assertTrue(drive.mutations.isEmpty())
            assertEquals("source", drive.nodes.getValue("save").parent)
        }
    }

    private fun migrator(drive: MigrationDrive) = CloudFolderMigrator(DriveFolders(drive.client), DriveSearch(drive.client))

    private fun orchestrator(
        drive: MigrationDrive,
        directory: File,
        entry: GameEntry,
        statuses: MutableList<AppStatus> = mutableListOf(),
        otherEntries: List<GameEntry> = emptyList(),
    ) = SyncOrchestrator(
        drive.client,
        AppConfig(GoogleDriveConfig("client", "secret", "refresh"), listOf(entry) + otherEntries),
        ConfigManager(File(directory, "config.json")),
        onStatus = { statuses += it },
    )
}

/** Stateful Drive substitute: assertions inspect actual resulting parents, not just HTTP calls. */
internal class MigrationDrive : AutoCloseable {
    data class Node(
        val id: String,
        var name: String,
        var parent: String?,
        val folder: Boolean,
        val content: String = "SAVE DATA",
    )
    data class Move(val id: String, val from: String?, val to: String?)

    val nodes = linkedMapOf("root" to Node("root", "EmuSync", null, true))
    val moves = mutableListOf<Move>()
    val deletions = mutableListOf<String>()
    val mutations = mutableListOf<String>()
    var failMove: (Move) -> Boolean = { false }
    var onMove: suspend (Move) -> Unit = {}

    fun folder(id: String, name: String, parent: String = "root") {
        nodes[id] = Node(id, name, parent, true)
    }
    fun file(id: String, name: String, parent: String, content: String = "SAVE DATA") {
        nodes[id] = Node(id, name, parent, false, content)
    }
    fun children(parent: String) = nodes.values.filter { it.parent == parent }

    val client = HttpClient(MockEngine { request ->
        val headers = headersOf(HttpHeaders.ContentType, "application/json")
        val id = request.url.segments.last()
        when {
            request.url.host == "oauth2.googleapis.com" -> respond(
                """{"access_token":"token","expires_in":3600,"token_type":"Bearer"}""", headers = headers,
            )
            request.method.value == "GET" && request.url.parameters["q"] != null -> {
                val query = request.url.parameters["q"]!!
                val name = Regex("name = '([^']*)'").find(query)?.groupValues?.get(1)
                val parent = Regex("'([^']*)' in parents").find(query)?.groupValues?.get(1)
                val foldersOnly = query.contains("mimeType = 'application/vnd.google-apps.folder'")
                val results = nodes.values.filter {
                    (name == null || it.name == name) && (parent == null || it.parent == parent) && (!foldersOnly || it.folder)
                }
                respond(buildJsonObject {
                    put("files", buildJsonArray {
                        results.forEach { node -> add(buildJsonObject {
                            put("id", node.id)
                            put("name", node.name)
                            put("modifiedTime", "2026-10-01T12:00:00Z")
                            put("mimeType", if (node.folder) "application/vnd.google-apps.folder" else "application/octet-stream")
                        }) }
                    })
                }.toString(), headers = headers)
            }
            request.method.value == "PATCH" -> {
                mutations += "PATCH:$id"
                val node = nodes.getValue(id)
                val to = request.url.parameters["addParents"]
                val from = request.url.parameters["removeParents"]
                if (to != null) {
                    val move = Move(id, from, to)
                    moves += move
                    onMove(move)
                    if (failMove(move)) return@MockEngine respond(
                        """{"error":"simulated move failure"}""", HttpStatusCode.ServiceUnavailable, headers,
                    )
                    check(node.parent == from) { "Unexpected source parent for $id" }
                    node.parent = to
                }
                val body = (request.body as? OutgoingContent.ByteArrayContent)?.bytes()?.decodeToString()
                if (!body.isNullOrBlank()) {
                    Json.parseToJsonElement(body).jsonObject["name"]?.jsonPrimitive?.content?.let { node.name = it }
                }
                respond("{}", headers = headers)
            }
            request.method.value == "DELETE" -> {
                deletions += id
                mutations += "DELETE:$id"
                nodes.remove(id)
                respond("", HttpStatusCode.NoContent)
            }
            else -> error("Unexpected Drive request: ${request.method} ${request.url}")
        }
    }) {
        install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
    }

    override fun close() = client.close()
}
