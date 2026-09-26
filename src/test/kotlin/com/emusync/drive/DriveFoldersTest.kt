package com.emusync.drive

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
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull

class DriveFoldersTest {

    private val jsonHeaders = headersOf(HttpHeaders.ContentType, "application/json")

    @Test
    fun `ensurePath creates nested folder hierarchy EmuSync to Switch to Game`() = runTest {
        val requests = mutableListOf<String>()

        val client = HttpClient(MockEngine) {
            install(ContentNegotiation) {
                json(Json { ignoreUnknownKeys = true })
            }
            engine {
                addHandler { request ->
                    val url = request.url.toString()
                    val q = request.url.parameters["q"] ?: ""
                    requests.add("$url | q=$q")

                    when {
                        // Root search EmuSync
                        q.contains("name = 'EmuSync'") -> {
                            respond(
                                content = """{"files":[{"id":"root-id","name":"EmuSync"}]}""",
                                headers = jsonHeaders,
                            )
                        }
                        // Switch search
                        q.contains("name = 'Switch'") -> {
                            respond(
                                content = """{"files":[{"id":"switch-id","name":"Switch"}]}""",
                                headers = jsonHeaders,
                            )
                        }
                        // Crash Team Racing search -> not found first time, then create
                        q.contains("name = 'Crash Team Racing Nitro-Fueled'") -> {
                            respond(
                                content = """{"files":[]}""",
                                headers = jsonHeaders,
                            )
                        }
                        // Create folder
                        request.method.value == "POST" -> {
                            respond(
                                content = """{"id":"game-folder-id","name":"Crash Team Racing Nitro-Fueled"}""",
                                status = HttpStatusCode.Created,
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

        val driveFolders = DriveFolders(client)
        val leafFolderId = driveFolders.ensurePath("test-token", listOf("Switch", "Crash Team Racing Nitro-Fueled"))

        assertEquals("game-folder-id", leafFolderId)

        // Subsequent call for the same path should use cache without additional network calls
        val requestCountBefore = requests.size
        val cachedLeafId = driveFolders.ensurePath("test-token", listOf("Switch", "Crash Team Racing Nitro-Fueled"))
        assertEquals("game-folder-id", cachedLeafId)
        assertEquals(requestCountBefore, requests.size, "Subsequent ensurePath should hit folderCache")
    }

    @Test
    fun `findPath returns leaf folder ID when path exists, and null when a folder is missing`() = runTest {
        val client = HttpClient(MockEngine) {
            install(ContentNegotiation) {
                json(Json { ignoreUnknownKeys = true })
            }
            engine {
                addHandler { request ->
                    val q = request.url.parameters["q"] ?: ""
                    when {
                        q.contains("name = 'EmuSync'") -> {
                            respond(
                                content = """{"files":[{"id":"root-id","name":"EmuSync"}]}""",
                                headers = jsonHeaders,
                            )
                        }
                        q.contains("name = 'Switch'") -> {
                            respond(
                                content = """{"files":[{"id":"switch-id","name":"Switch"}]}""",
                                headers = jsonHeaders,
                            )
                        }
                        q.contains("name = 'ExistingGame'") -> {
                            respond(
                                content = """{"files":[{"id":"existing-game-id","name":"ExistingGame"}]}""",
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

        val driveFolders = DriveFolders(client)

        val existingId = driveFolders.findPath("test-token", listOf("Switch", "ExistingGame"))
        assertNotNull(existingId)
        assertEquals("existing-game-id", existingId)

        val nonExistingId = driveFolders.findPath("test-token", listOf("Switch", "MissingGame"))
        assertNull(nonExistingId)
    }

    @Test
    fun `searchFolder properly escapes single quotes in game titles`() = runTest {
        val queries = mutableListOf<String>()

        val client = HttpClient(MockEngine) {
            install(ContentNegotiation) {
                json(Json { ignoreUnknownKeys = true })
            }
            engine {
                addHandler { request ->
                    val q = request.url.parameters["q"] ?: ""
                    queries.add(q)
                    when {
                        q.contains("name = 'EmuSync'") -> {
                            respond(
                                content = """{"files":[{"id":"root-id","name":"EmuSync"}]}""",
                                headers = jsonHeaders,
                            )
                        }
                        else -> {
                            respond(
                                content = """{"files":[{"id":"mock-id","name":"Tony Hawk's Pro Skater"}]}""",
                                headers = jsonHeaders,
                            )
                        }
                    }
                }
            }
        }

        val driveFolders = DriveFolders(client)
        driveFolders.findPath("test-token", listOf("Tony Hawk's Pro Skater"))

        // Verify single quote is escaped with \' in the Drive API query string
        val tonyQuery = queries.find { it.contains("Tony Hawk") }
        assertNotNull(tonyQuery, "Expected a query containing Tony Hawk")
        org.junit.jupiter.api.Assertions.assertTrue(
            tonyQuery.contains("name = 'Tony Hawk\\'s Pro Skater'"),
            "Query should escape single quote: $tonyQuery"
        )
    }

    @Test
    fun `renameFolder sends PATCH with new name and parent parameters`() = runTest {
        var patchUrl: String? = null
        var addParentsParam: String? = null
        var removeParentsParam: String? = null

        val client = HttpClient(MockEngine) {
            install(ContentNegotiation) {
                json(Json { ignoreUnknownKeys = true })
            }
            engine {
                addHandler { request ->
                    patchUrl = request.url.toString()
                    addParentsParam = request.url.parameters["addParents"]
                    removeParentsParam = request.url.parameters["removeParents"]
                    respond(
                        content = """{"id":"folder-123","name":"New Game Name"}""",
                        headers = jsonHeaders,
                    )
                }
            }
        }

        val driveFolders = DriveFolders(client)
        driveFolders.renameFolder(
            accessToken = "token",
            folderId = "folder-123",
            newName = "New Game Name",
            addParentId = "parent-new",
            removeParentId = "parent-old",
        )

        assertNotNull(patchUrl)
        org.junit.jupiter.api.Assertions.assertTrue(patchUrl!!.contains("files/folder-123"))
        assertEquals("parent-new", addParentsParam)
        assertEquals("parent-old", removeParentsParam)
    }

    @Test
    fun `deleteFolder sends DELETE request to folder endpoint`() = runTest {
        var deleteMethod: String? = null
        var deleteUrl: String? = null

        val client = HttpClient(MockEngine) {
            engine {
                addHandler { request ->
                    deleteMethod = request.method.value
                    deleteUrl = request.url.toString()
                    respond("", status = HttpStatusCode.NoContent)
                }
            }
        }

        val driveFolders = DriveFolders(client)
        driveFolders.deleteFolder("token", "folder-to-delete")

        assertEquals("DELETE", deleteMethod)
        org.junit.jupiter.api.Assertions.assertTrue(deleteUrl!!.contains("files/folder-to-delete"))
    }
}
