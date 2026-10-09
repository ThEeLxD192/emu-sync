package com.emusync.drive

import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.get
import io.ktor.client.request.header
import io.ktor.client.request.parameter
import io.ktor.client.request.patch
import io.ktor.client.request.post
import io.ktor.client.request.put
import io.ktor.client.request.setBody
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.contentType
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import java.io.File
import java.io.RandomAccessFile
import java.nio.file.AtomicMoveNotSupportedException
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.time.Instant

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
        private const val UPLOAD_CHUNK_SIZE = 8 * 1024 * 1024
        private const val UPLOAD_FIELDS = "id, name, modifiedTime"
    }

    /**
     * Downloads a file from Google Drive and saves it to a local path.
     *
     * @param accessToken  Valid OAuth 2.0 access token.
     * @param fileId       Google Drive file ID to download.
     * @param destination  Local [File] path where the content will be written.
     * @param modifiedTime Optional ISO 8601 modifiedTime from Drive to synchronize local timestamp.
     * @throws DriveApiException if the download fails.
     */
    suspend fun download(
        accessToken: String,
        fileId: String,
        destination: File,
        modifiedTime: String? = null,
    ) {
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
            val parent = destination.absoluteFile.parentFile
                ?: throw DriveApiException("Could not resolve destination directory for $destination")
            if (!parent.exists() && !parent.mkdirs()) {
                throw DriveApiException("Could not create destination directory: $parent")
            }

            var temporaryFile: java.nio.file.Path? = null
            try {
                temporaryFile = Files.createTempFile(parent.toPath(), ".${destination.name}.", ".part")
                Files.write(temporaryFile, bytes)
                try {
                    Files.move(
                        temporaryFile,
                        destination.toPath(),
                        StandardCopyOption.ATOMIC_MOVE,
                        StandardCopyOption.REPLACE_EXISTING,
                    )
                } catch (_: AtomicMoveNotSupportedException) {
                    Files.move(temporaryFile, destination.toPath(), StandardCopyOption.REPLACE_EXISTING)
                }
                temporaryFile = null

                if (modifiedTime != null) {
                    try {
                        destination.setLastModified(Instant.parse(modifiedTime).toEpochMilli())
                    } catch (_: Exception) {
                        // Ignore timestamp setting failure
                    }
                }
            } finally {
                temporaryFile?.let { Files.deleteIfExists(it) }
            }
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
        val sessionUri = startResumableUpload(
            accessToken = accessToken,
            localFile = localFile,
            driveName = driveName,
            parentFolderId = parentFolderId,
        )
        return uploadResumableContent(accessToken, localFile, sessionUri)
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
        val sessionUri = startResumableUpload(
            accessToken = accessToken,
            localFile = localFile,
            fileId = fileId,
        )
        return uploadResumableContent(accessToken, localFile, sessionUri)
    }

    private suspend fun startResumableUpload(
        accessToken: String,
        localFile: File,
        driveName: String = localFile.name,
        parentFolderId: String? = null,
        fileId: String? = null,
    ): String {
        val metadata = if (fileId == null) {
            val escapedName = driveName.replace("\\", "\\\\").replace("\"", "\\\"")
            val parentsJson = parentFolderId?.let { ", \"parents\": [\"$it\"]" } ?: ""
            "{\"name\": \"$escapedName\"$parentsJson}"
        } else {
            "{}"
        }

        val requestUrl = if (fileId == null) UPLOAD_URL else "$UPLOAD_URL/$fileId"
        val response = if (fileId == null) {
            client.post(requestUrl) {
                configureResumableSession(accessToken, localFile, metadata)
            }
        } else {
            client.patch(requestUrl) {
                configureResumableSession(accessToken, localFile, metadata)
            }
        }

        if (response.status != HttpStatusCode.OK && response.status != HttpStatusCode.Created) {
            val errorBody = response.body<String>()
            throw DriveApiException("Drive upload session creation failed (${response.status}): $errorBody")
        }

        return response.headers[HttpHeaders.Location]
            ?: throw DriveApiException("Drive upload session response did not include a Location header")
    }

    private fun io.ktor.client.request.HttpRequestBuilder.configureResumableSession(
        accessToken: String,
        localFile: File,
        metadata: String,
    ) {
        header(HttpHeaders.Authorization, "Bearer $accessToken")
        header("X-Upload-Content-Type", "application/octet-stream")
        header("X-Upload-Content-Length", localFile.length().toString())
        parameter("uploadType", "resumable")
        parameter("fields", UPLOAD_FIELDS)
        contentType(ContentType.Application.Json)
        setBody(metadata)
    }

    private suspend fun uploadResumableContent(
        accessToken: String,
        localFile: File,
        sessionUri: String,
    ): DriveUploadResponse = withContext(Dispatchers.IO) {
        val totalBytes = localFile.length()
        var offset = 0L

        RandomAccessFile(localFile, "r").use { source ->
            while (offset < totalBytes || (totalBytes == 0L && offset == 0L)) {
                val chunkLength = minOf(UPLOAD_CHUNK_SIZE.toLong(), totalBytes - offset).toInt()
                val chunk = ByteArray(chunkLength)
                if (chunkLength > 0) {
                    source.seek(offset)
                    source.readFully(chunk)
                }

                val response = client.put(sessionUri) {
                    header(HttpHeaders.Authorization, "Bearer $accessToken")
                    header(HttpHeaders.ContentLength, chunkLength.toString())
                    if (totalBytes > 0) {
                        header(HttpHeaders.ContentRange, "bytes $offset-${offset + chunkLength - 1}/$totalBytes")
                    }
                    contentType(ContentType.Application.OctetStream)
                    setBody(chunk)
                }

                when {
                    response.status == HttpStatusCode.OK || response.status == HttpStatusCode.Created -> {
                        return@withContext response.body<DriveUploadResponse>()
                    }
                    response.status.value == 308 -> {
                        val receivedThrough = response.headers[HttpHeaders.Range]
                            ?.substringAfterLast('-')
                            ?.toLongOrNull()
                        val nextOffset = receivedThrough?.plus(1) ?: 0L
                        if (nextOffset <= offset || nextOffset > totalBytes) {
                            throw DriveApiException("Drive upload made no progress at byte $offset")
                        }
                        offset = nextOffset
                    }
                    else -> {
                        val errorBody = response.body<String>()
                        throw DriveApiException("Drive upload failed (${response.status}): $errorBody")
                    }
                }

                if (totalBytes == 0L) break
            }
        }

        throw DriveApiException("Drive upload ended without a completed file response")
    }
}
