package com.emusync.drive

import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.get
import io.ktor.client.request.header
import io.ktor.client.request.parameter
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
        val rootId = findOrCreateFolder(accessToken, ROOT_FOLDER_NAME, parentId = null)
        return findOrCreateFolder(accessToken, entryName, parentId = rootId)
    }

    /**
     * Finds a folder by name (optionally inside a parent), or creates it.
     */
    private suspend fun findOrCreateFolder(
        accessToken: String,
        folderName: String,
        parentId: String?,
    ): String {
        // Search for existing folder
        val existing = searchFolder(accessToken, folderName, parentId)
        if (existing != null) return existing.id

        // Create the folder
        return createFolder(accessToken, folderName, parentId)
    }

    /**
     * Searches for a folder by exact name, optionally under a parent folder.
     */
    private suspend fun searchFolder(
        accessToken: String,
        folderName: String,
        parentId: String?,
    ): FolderInfo? {
        val query = buildString {
            append("name = '$folderName'")
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
        return list.files.firstOrNull()
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
        val metadata = """{"name": "$folderName", "mimeType": "$FOLDER_MIME", "parents": $parentsJson}"""

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
        return created.id
    }
}
