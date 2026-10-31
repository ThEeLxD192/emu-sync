package com.emusync.drive

import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.content.OutgoingContent
import io.ktor.http.headersOf
import io.ktor.serialization.kotlinx.json.json
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/** Stateful Drive substitute: assertions inspect actual resulting parents, not just HTTP calls. */
class MigrationDrive : AutoCloseable {
    val nodes = linkedMapOf("root" to MigrationNode("root", "EmuSync", null, true))
    val moves = mutableListOf<MigrationMove>()
    val deletions = mutableListOf<String>()
    val mutations = mutableListOf<String>()
    var failMove: (MigrationMove) -> Boolean = { false }
    var onMove: suspend (MigrationMove) -> Unit = {}

    fun folder(id: String, name: String, parent: String = "root") {
        nodes[id] = MigrationNode(id, name, parent, true)
    }

    fun file(id: String, name: String, parent: String, content: String = "SAVE DATA") {
        nodes[id] = MigrationNode(id, name, parent, false, content)
    }

    fun children(parent: String) = nodes.values.filter { it.parent == parent }

    val client =
        HttpClient(
            MockEngine { request ->
                val headers = headersOf(HttpHeaders.ContentType, "application/json")
                val id = request.url.segments.last()
                when {
                    request.url.host == "oauth2.googleapis.com" ->
                        respond(
                            """{"access_token":"token","expires_in":3600,"token_type":"Bearer"}""",
                            headers = headers,
                        )
                    request.method.value == "GET" && request.url.parameters["q"] != null -> {
                        val query = checkNotNull(request.url.parameters["q"])
                        val name = Regex("name = '([^']*)'").find(query)?.groupValues?.get(1)
                        val parent = Regex("'([^']*)' in parents").find(query)?.groupValues?.get(1)
                        val foldersOnly =
                            query.contains("mimeType = 'application/vnd.google-apps.folder'")
                        val results =
                            nodes.values.filter {
                                (name == null || it.name == name) &&
                                    (parent == null || it.parent == parent) &&
                                    (!foldersOnly || it.folder)
                            }
                        respond(
                            buildJsonObject {
                                put(
                                    "files",
                                    buildJsonArray {
                                        results.forEach { node ->
                                            add(
                                                buildJsonObject {
                                                    put("id", node.id)
                                                    put("name", node.name)
                                                    put("modifiedTime", "2026-10-01T12:00:00Z")
                                                    put(
                                                        "mimeType",
                                                        if (node.folder)
                                                            "application/vnd.google-apps.folder"
                                                        else "application/octet-stream",
                                                    )
                                                }
                                            )
                                        }
                                    },
                                )
                            }
                                .toString(),
                            headers = headers,
                        )
                    }
                    request.method.value == "PATCH" -> {
                        mutations += "PATCH:$id"
                        val node = nodes.getValue(id)
                        val to = request.url.parameters["addParents"]
                        val from = request.url.parameters["removeParents"]
                        if (to != null) {
                            val move = MigrationMove(id, from, to)
                            moves += move
                            onMove(move)
                            if (failMove(move))
                                return@MockEngine respond(
                                    """{"error":"simulated move failure"}""",
                                    HttpStatusCode.ServiceUnavailable,
                                    headers,
                                )
                            check(node.parent == from) { "Unexpected source parent for $id" }
                            node.parent = to
                        }
                        val body =
                            (request.body as? OutgoingContent.ByteArrayContent)
                                ?.bytes()
                                ?.decodeToString()
                        if (!body.isNullOrBlank()) {
                            Json.parseToJsonElement(body)
                                .jsonObject["name"]
                                ?.jsonPrimitive
                                ?.content
                                ?.let { node.name = it }
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
            }
        ) {
            install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) }
        }

    override fun close() = client.close()
}
