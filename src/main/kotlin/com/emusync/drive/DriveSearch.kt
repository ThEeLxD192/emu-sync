package com.emusync.drive

import io.ktor.client.*
import io.ktor.client.call.*
import io.ktor.client.request.*
import io.ktor.http.*
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * A single file result from the Google Drive Files.list API.
 */
@Serializable
data class DriveFile(
    val id: String,
    val name: String,
    @SerialName("modifiedTime") val modifiedTime: String,  // ISO 8601 e.g. "2024-01-15T10:30:00.000Z"
)

/**
 * Google Drive Files.list API response.
 */
@Serializable
data class DriveFileList(
    val files: List<DriveFile> = emptyList(),
)

/**
 * Searches for files on Google Drive by exact name.
 *
 * Uses the Drive API v3 `files.list` endpoint with a query filter.
 */
class DriveSearch(private val client: HttpClient) {

    companion object {
        private const val FILES_URL = "https://www.googleapis.com/drive/v3/files"
    }

    /**
     * Searches for a file on Google Drive by its exact name.
     *
     * @param accessToken    Valid OAuth 2.0 access token.
     * @param fileName       Exact name of the file to find (e.g. "pokemon_emerald.sav").
     * @param parentFolderId Optional folder ID to scope the search (e.g. the entry's subfolder).
     * @return [DriveFile] if found, or `null` if no file matches.
     *         If multiple files match, returns the most recently modified one.
     * @throws DriveApiException if the API request fails.
     */
    suspend fun findByName(
        accessToken: String,
        fileName: String,
        parentFolderId: String? = null,
    ): DriveFile? {
        val query = buildString {
            append("name = '$fileName' and trashed = false")
            if (parentFolderId != null) {
                append(" and '$parentFolderId' in parents")
            }
        }

        val response = client.get(FILES_URL) {
            header(HttpHeaders.Authorization, "Bearer $accessToken")
            parameter("q", query)
            parameter("fields", "files(id, name, modifiedTime)")
            parameter("orderBy", "modifiedTime desc")
            parameter("pageSize", "1")
        }

        if (response.status != HttpStatusCode.OK) {
            val errorBody = response.body<String>()
            throw DriveApiException("Drive search failed (${response.status}): $errorBody")
        }

        val fileList = response.body<DriveFileList>()
        return fileList.files.firstOrNull()
    }

    /**
     * Lists all files in a specific Google Drive folder.
     *
     * @param accessToken Valid OAuth 2.0 access token.
     * @param folderId    Google Drive folder ID.
     * @return List of [DriveFile] objects.
     */
    suspend fun listFilesInFolder(
        accessToken: String,
        folderId: String,
    ): List<DriveFile> {
        val query = "'$folderId' in parents and trashed = false"

        val response = client.get(FILES_URL) {
            header(HttpHeaders.Authorization, "Bearer $accessToken")
            parameter("q", query)
            parameter("fields", "files(id, name, modifiedTime)")
            parameter("orderBy", "modifiedTime desc")
        }

        if (response.status != HttpStatusCode.OK) {
            val errorBody = response.body<String>()
            throw DriveApiException("Drive list files failed (${response.status}): $errorBody")
        }

        return response.body<DriveFileList>().files
    }

    /**
     * Retrieves a file's metadata by its Drive ID.
     *
     * @param accessToken Valid OAuth 2.0 access token.
     * @param fileId      Google Drive file ID.
     * @return [DriveFile] with id, name, and modifiedTime.
     * @throws DriveApiException if the file is not found or the request fails.
     */
    suspend fun getById(accessToken: String, fileId: String): DriveFile {
        val response = client.get("$FILES_URL/$fileId") {
            header(HttpHeaders.Authorization, "Bearer $accessToken")
            parameter("fields", "id, name, modifiedTime")
        }

        if (response.status != HttpStatusCode.OK) {
            val errorBody = response.body<String>()
            throw DriveApiException("Drive get by ID failed (${response.status}): $errorBody")
        }

        return response.body<DriveFile>()
    }
}
