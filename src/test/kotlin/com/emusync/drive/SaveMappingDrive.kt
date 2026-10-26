package com.emusync.drive

import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.toByteArray
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
import java.time.Instant

internal class SaveMappingDrive : AutoCloseable {
    companion object { const val CLOUD_TIME = "2026-10-01T12:00:00Z" }
    val files = linkedMapOf<String, DriveFile>()
    val contents = mutableMapOf<String, String>()
    val folderQueries = mutableListOf<String>()
    val downloads = mutableListOf<String>()
    val startedNames = mutableListOf<String>()
    val uploadedNames = mutableListOf<String>()
    val failOnce = mutableSetOf<String>()
    var beforeDownload: () -> Unit = {}
    var beforeUpload: () -> Unit = {}
    var beforeConditionalCommit: () -> Unit = {}
    private val sessions = mutableMapOf<String, Pair<String, String>>()

    fun add(id: String, name: String, content: String) {
        files[id] = DriveFile(id, name, CLOUD_TIME, sha256Checksum = sha256(content.toByteArray()), version = "1")
        contents[id] = content
    }

    val client = HttpClient(MockEngine { request ->
        val headers = headersOf(HttpHeaders.ContentType, "application/json")
        val id = request.url.segments.last()
        val query = request.url.parameters["q"].orEmpty()
        when {
            request.url.host == "oauth2.googleapis.com" -> respond(
                """{"access_token":"token","expires_in":3600,"token_type":"Bearer"}""", headers = headers,
            )
            request.url.encodedPath.startsWith("/drive/v2/files/") -> {
                val file = files.getValue(id)
                respondConditional(file)
            }
            request.url.encodedPath.startsWith("/upload/drive/v2/files/") -> {
                val file = files.getValue(id)
                startedNames += file.name
                if (failOnce.remove(file.name)) return@MockEngine respond("simulated failure", HttpStatusCode.ServiceUnavailable)
                beforeUpload()
                beforeConditionalCommit()
                if (request.headers[HttpHeaders.IfMatch] != etag(files.getValue(id))) {
                    return@MockEngine respond("changed", HttpStatusCode.PreconditionFailed)
                }
                contents[id] = request.body.toByteArray().decodeToString()
                files[id] = file.copy(modifiedTime = Instant.now().toString(),
                    version = ((file.version?.toLongOrNull() ?: 0) + 1).toString(),
                    sha256Checksum = sha256(contents.getValue(id).toByteArray()))
                uploadedNames += file.name
                respondConditional(files.getValue(id))
            }
            request.url.parameters["alt"] == "media" -> {
                beforeDownload()
                downloads += id
                respond(contents.getValue(id), headers = headersOf(HttpHeaders.ContentType, "application/octet-stream"))
            }
            request.url.encodedPath.contains("/upload/drive/v3/files") -> {
                val body = (request.body as OutgoingContent.ByteArrayContent).bytes().decodeToString()
                val updating = request.method.value == "PATCH"
                val name = if (updating) files.getValue(id).name else Json.parseToJsonElement(body).jsonObject.getValue("name").jsonPrimitive.content
                startedNames += name
                if (failOnce.remove(name)) return@MockEngine respond("simulated failure", HttpStatusCode.ServiceUnavailable)
                val session = "session-${sessions.size}"
                sessions[session] = (if (updating) id else "created-${files.size}") to name
                respond("", headers = headersOf(HttpHeaders.Location, "https://uploads.example/$session"))
            }
            request.method.value == "PUT" -> {
                beforeUpload()
                val (fileId, name) = sessions.getValue(id)
                contents[fileId] = (request.body as OutgoingContent.ByteArrayContent).bytes().decodeToString()
                files[fileId] = DriveFile(fileId, name, Instant.now().toString(),
                    sha256Checksum = sha256(contents.getValue(fileId).toByteArray()),
                    version = ((files[fileId]?.version?.toLongOrNull() ?: 0) + 1).toString())
                uploadedNames += name
                respond(Json.encodeToString(DriveFile.serializer(), files.getValue(fileId)), headers = headers)
            }
            query.contains("mimeType = 'application/vnd.google-apps.folder'") -> {
                folderQueries += query
                val root = query.contains("name = 'EmuSync'")
                respond("""{"files":[{"id":"${if (root) "root" else "game"}","name":"${if (root) "EmuSync" else "Game"}"}]}""", headers = headers)
            }
            query.contains("'game' in parents") -> respond(buildJsonObject {
                put("files", buildJsonArray { files.values.forEach { add(Json.parseToJsonElement(Json.encodeToString(DriveFile.serializer(), it))) } })
            }.toString(), headers = headers)
            else -> error("Unexpected request: ${request.method} ${request.url}")
        }
    }) { install(ContentNegotiation) { json(Json { ignoreUnknownKeys = true }) } }

    private fun etag(file: DriveFile) = "\"${file.id}:${file.version}:${file.sha256Checksum}\""
    private fun io.ktor.client.engine.mock.MockRequestHandleScope.respondConditional(file: DriveFile) = respond(
        Json.encodeToString(ConditionalDriveFile.serializer(), ConditionalDriveFile(file.id, file.name,
            file.modifiedTime, etag(file), file.version, file.sha256Checksum)),
        headers = headersOf(HttpHeaders.ContentType, "application/json"))

    override fun close() = client.close()
}
