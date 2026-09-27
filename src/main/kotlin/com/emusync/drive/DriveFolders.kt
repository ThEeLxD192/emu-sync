package com.emusync.drive

import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.delete
import io.ktor.client.request.get
import io.ktor.client.request.header
import io.ktor.client.request.parameter
import io.ktor.client.request.patch
import io.ktor.client.request.post
import io.ktor.client.request.setBody
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.contentType
import kotlinx.serialization.Serializable

/**
 * Manages the folder hierarchy on Google Drive for EmuSync saves.
 *
 * Structure on Drive:
 * ```
 * EmuSync/                       ← root folder (created automatically)
 * ├── Game Boy Advance/          ← one folder per GameEntry.name
 * │   ├── pokemon_emerald.sav
 * │   └── metroid_fusion.sav
 * ├── Nintendo DS/
 * │   └── pokemon_platinum.sav
 * ├── Spelunky Classic/
 * │   └── save.dat
 * └── Cave Story/
 *     └── Profile.dat
 * ```
 */
class DriveFolders(private val client: HttpClient) {

    companion object {
        private const val FILES_URL = "https://www.googleapis.com/drive/v3/files"
        private const val FOLDER_MIME = "application/vnd.google-apps.folder"
        const val ROOT_FOLDER_NAME = "EmuSync"
    }

    /**
     * Cache mapping "(parentId ?: 'root'):folderName" to Drive folder ID.
     */
    private val folderCache = java.util.concurrent.ConcurrentHashMap<String, String>()

    /**
     * Response from folder search.
     */
    @Serializable
    private data class FolderList(val files: List<FolderInfo> = emptyList())

    @Serializable
    data class FolderInfo(val id: String, val name: String)

    /**
     * Ensures the full folder path exists on Drive: `EmuSync/<entryName>/`.
     * Creates any missing folders.
     *
     * @param accessToken Valid access token.
     * @param entryName   The GameEntry.name (e.g. "Game Boy Advance").
     * @return The Drive folder ID of the entry's subfolder.
     */
    suspend fun ensureEntryFolder(accessToken: String, entryName: String): String {
        return ensurePath(accessToken, listOf(entryName))
    }

    /**
     * Ensures the full hierarchical folder path exists on Drive starting from [ROOT_FOLDER_NAME].
     * E.g. `ensurePath(token, listOf("Switch", "Crash Team Racing Nitro-Fueled"))`
     * will create or find: EmuSync -> Switch -> Crash Team Racing Nitro-Fueled
     *
     * @param accessToken Valid access token.
     * @param pathSegments List of folder names representing the hierarchy under [ROOT_FOLDER_NAME].
     * @return The Drive folder ID of the leaf subfolder.
     */
    suspend fun ensurePath(accessToken: String, pathSegments: List<String>): String {
        var currentParentId = findOrCreateFolder(accessToken, ROOT_FOLDER_NAME, parentId = null)
        for (segment in pathSegments) {
            val trimmed = segment.trim()
            if (trimmed.isNotBlank()) {
                currentParentId = findOrCreateFolder(accessToken, trimmed, parentId = currentParentId)
            }
        }
        return currentParentId
    }

    /**
     * Finds the folder ID for the given hierarchical path starting from [ROOT_FOLDER_NAME],
     * or returns null if any segment in the path does not exist on Drive.
     */
    suspend fun findPath(accessToken: String, pathSegments: List<String>): String? {
        val root = searchFolder(accessToken, ROOT_FOLDER_NAME, parentId = null) ?: return null
        var currentParentId = root.id
        for (segment in pathSegments) {
            val trimmed = segment.trim()
            if (trimmed.isNotBlank()) {
                val found = searchFolder(accessToken, trimmed, parentId = currentParentId) ?: return null
                currentParentId = found.id
            }
        }
        return currentParentId
    }

    /**
     * Finds a folder by name (optionally inside a parent), or creates it.
     */
    private suspend fun findOrCreateFolder(
        accessToken: String,
        folderName: String,
        parentId: String?,
    ): String {
        val cacheKey = "${parentId ?: "root"}:$folderName"
        folderCache[cacheKey]?.let { return it }

        // Search for existing folder
        val existing = searchFolder(accessToken, folderName, parentId)
        if (existing != null) {
            folderCache[cacheKey] = existing.id
            return existing.id
        }

        // Create the folder
        val createdId = createFolder(accessToken, folderName, parentId)
        folderCache[cacheKey] = createdId
        return createdId
    }

    /**
     * Searches for a folder by exact name, optionally under a parent folder.
     */
    private suspend fun searchFolder(
        accessToken: String,
        folderName: String,
        parentId: String?,
    ): FolderInfo? {
        val cacheKey = "${parentId ?: "root"}:$folderName"
        folderCache[cacheKey]?.let { return FolderInfo(it, folderName) }

        val escapedName = folderName.replace("'", "\\'")
        val query = buildString {
            append("name = '$escapedName'")
            append(" and mimeType = '$FOLDER_MIME'")
            append(" and trashed = false")
            if (parentId != null) {
                append(" and '$parentId' in parents")
            }
        }

        val response = client.get(FILES_URL) {
            header(HttpHeaders.Authorization, "Bearer $accessToken")
            parameter("q", query)
            parameter("fields", "files(id, name)")
            parameter("pageSize", "1")
        }

        if (response.status != HttpStatusCode.OK) {
            val errorBody = response.body<String>()
            throw DriveApiException("Folder search failed (${response.status}): $errorBody")
        }

        val list = response.body<FolderList>()
        val found = list.files.firstOrNull()
        if (found != null) {
            folderCache[cacheKey] = found.id
        }
        return found
    }

    /**
     * Creates a new folder on Drive.
     */
    private suspend fun createFolder(
        accessToken: String,
        folderName: String,
        parentId: String?,
    ): String {
        val parentsJson = if (parentId != null) """["$parentId"]""" else "[]"
        val escapedJsonName = folderName.replace("\\", "\\\\").replace("\"", "\\\"")
        val metadata = """{"name": "$escapedJsonName", "mimeType": "$FOLDER_MIME", "parents": $parentsJson}"""

        val response = client.post(FILES_URL) {
            header(HttpHeaders.Authorization, "Bearer $accessToken")
            contentType(ContentType.Application.Json)
            setBody(metadata)
        }

        if (response.status != HttpStatusCode.OK && response.status != HttpStatusCode.Created) {
            val errorBody = response.body<String>()
            throw DriveApiException("Folder creation failed (${response.status}): $errorBody")
        }

        val created = response.body<FolderInfo>()
        val cacheKey = "${parentId ?: "root"}:$folderName"
        folderCache[cacheKey] = created.id
        return created.id
    }

    /**
     * Renames and/or moves a folder on Google Drive.
     *
     * @param accessToken Valid access token.
     * @param folderId    ID of the folder to rename/move.
     * @param newName     New name of the folder.
     * @param addParentId Optional new parent folder ID to add.
     * @param removeParentId Optional old parent folder ID to remove.
     */
    suspend fun renameFolder(
        accessToken: String,
        folderId: String,
        newName: String,
        addParentId: String? = null,
        removeParentId: String? = null,
    ) {
        val escapedJsonName = newName.replace("\\", "\\\\").replace("\"", "\\\"")
        val metadata = """{"name": "$escapedJsonName"}"""

        val response = client.patch("$FILES_URL/$folderId") {
            header(HttpHeaders.Authorization, "Bearer $accessToken")
            contentType(ContentType.Application.Json)
            if (addParentId != null) {
                parameter("addParents", addParentId)
            }
            if (removeParentId != null) {
                parameter("removeParents", removeParentId)
            }
            setBody(metadata)
        }

        if (response.status != HttpStatusCode.OK) {
            val errorBody = response.body<String>()
            throw DriveApiException("Folder rename failed (${response.status}): $errorBody")
        }

        folderCache.clear()
    }

    /**
     * Moves a file between folders on Google Drive.
     */
    suspend fun moveFile(
        accessToken: String,
        fileId: String,
        addParentId: String,
        removeParentId: String,
    ) {
        val response = client.patch("$FILES_URL/$fileId") {
            header(HttpHeaders.Authorization, "Bearer $accessToken")
            parameter("addParents", addParentId)
            parameter("removeParents", removeParentId)
        }

        if (response.status != HttpStatusCode.OK) {
            val errorBody = response.body<String>()
            throw DriveApiException("Move file failed (${response.status}): $errorBody")
        }
    }

    /**
     * Deletes or trashes a folder on Google Drive.
     */
    suspend fun deleteFolder(accessToken: String, folderId: String) {
        val response = client.delete("$FILES_URL/$folderId") {
            header(HttpHeaders.Authorization, "Bearer $accessToken")
        }

        if (response.status != HttpStatusCode.OK && response.status != HttpStatusCode.NoContent) {
            val errorBody = response.body<String>()
            throw DriveApiException("Delete folder failed (${response.status}): $errorBody")
        }

        folderCache.clear()
    }

    fun clearCache() {
        folderCache.clear()
    }
}
