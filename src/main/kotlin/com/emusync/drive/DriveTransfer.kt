package com.emusync.drive

import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.forms.MultiPartFormDataContent
import io.ktor.client.request.forms.formData
import io.ktor.client.request.get
import io.ktor.client.request.header
import io.ktor.client.request.parameter
import io.ktor.client.request.patch
import io.ktor.client.request.post
import io.ktor.client.request.setBody
import io.ktor.http.ContentType
import io.ktor.http.Headers
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.contentType
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import java.io.File

/**
 * Response from Drive upload (create or update).
 */
@Serializable
data class DriveUploadResponse(
    val id: String,
    val name: String,
    @SerialName("modifiedTime") val modifiedTime: String,
)

/**
 * Handles file download from and upload to Google Drive.
 *
 * - Download: GET file content by ID → write bytes to local file.
 * - Upload (create): POST new file to Drive.
 * - Upload (update): PATCH existing file on Drive.
 */
class DriveTransfer(private val client: HttpClient) {

    companion object {
        private const val DOWNLOAD_URL = "https://www.googleapis.com/drive/v3/files"
        private const val UPLOAD_URL = "https://www.googleapis.com/upload/drive/v3/files"
    }

    /**
     * Downloads a file from Google Drive and saves it to a local path.
     *
     * @param accessToken Valid OAuth 2.0 access token.
     * @param fileId      Google Drive file ID to download.
     * @param destination Local [File] path where the content will be written.
     * @throws DriveApiException if the download fails.
     */
    suspend fun download(accessToken: String, fileId: String, destination: File) {
        val response = client.get("$DOWNLOAD_URL/$fileId") {
            header(HttpHeaders.Authorization, "Bearer $accessToken")
            parameter("alt", "media")
        }

        if (response.status != HttpStatusCode.OK) {
            val errorBody = response.body<String>()
            throw DriveApiException("Drive download failed (${response.status}): $errorBody")
        }

        val bytes: ByteArray = response.body()
        withContext(Dispatchers.IO) {
            destination.parentFile?.mkdirs()
            destination.writeBytes(bytes)
        }
    }

    /**
     * Uploads a local file to Google Drive as a NEW file.
     *
     * @param accessToken    Valid OAuth 2.0 access token.
     * @param localFile      Local [File] to upload.
     * @param driveName      Name for the file on Drive (defaults to the local filename).
     * @param parentFolderId Optional folder ID to upload into (e.g. the entry's subfolder).
     * @return [DriveUploadResponse] with the new file's ID and metadata.
     * @throws DriveApiException if the upload fails.
     */
    suspend fun upload(
        accessToken: String,
        localFile: File,
        driveName: String = localFile.name,
        parentFolderId: String? = null,
    ): DriveUploadResponse {
        val fileBytes = withContext(Dispatchers.IO) { localFile.readBytes() }

        val parentsJson = if (parentFolderId != null) """, "parents": ["$parentFolderId"]""" else ""
        val metadataJson = """{"name": "$driveName"$parentsJson}"""

        val response = client.post(UPLOAD_URL) {
            header(HttpHeaders.Authorization, "Bearer $accessToken")
            parameter("uploadType", "multipart")
            parameter("fields", "id, name, modifiedTime")

            setBody(
                MultiPartFormDataContent(
                    formData {
                        // Part 1: metadata (JSON)
                        append("metadata", metadataJson,
                            Headers.build {
                                append(HttpHeaders.ContentType, "application/json")
                            }
                        )
                        // Part 2: file content
                        append("file", fileBytes,
                            Headers.build {
                                append(HttpHeaders.ContentType, "application/octet-stream")
                                append(HttpHeaders.ContentDisposition, "filename=\"$driveName\"")
                            }
                        )
                    }
                )
            )
        }

        if (response.status != HttpStatusCode.OK && response.status != HttpStatusCode.Created) {
            val errorBody = response.body<String>()
            throw DriveApiException("Drive upload failed (${response.status}): $errorBody")
        }

        return response.body<DriveUploadResponse>()
    }

    /**
     * Updates an existing file on Google Drive with new content.
     *
     * @param accessToken Valid OAuth 2.0 access token.
     * @param fileId      ID of the existing file on Drive to update.
     * @param localFile   Local [File] with the new content.
     * @return [DriveUploadResponse] with updated metadata.
     * @throws DriveApiException if the update fails.
     */
    suspend fun update(
        accessToken: String,
        fileId: String,
        localFile: File,
    ): DriveUploadResponse {
        val fileBytes = withContext(Dispatchers.IO) { localFile.readBytes() }

        val response = client.patch("$UPLOAD_URL/$fileId") {
            header(HttpHeaders.Authorization, "Bearer $accessToken")
            parameter("uploadType", "media")
            parameter("fields", "id, name, modifiedTime")
            contentType(ContentType.Application.OctetStream)
            setBody(fileBytes)
        }

        if (response.status != HttpStatusCode.OK) {
            val errorBody = response.body<String>()
            throw DriveApiException("Drive update failed (${response.status}): $errorBody")
        }

        return response.body<DriveUploadResponse>()
    }
}
