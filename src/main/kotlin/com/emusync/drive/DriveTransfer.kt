package com.emusync.drive

import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.plugins.HttpTimeoutConfig
import io.ktor.client.plugins.timeout
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
import io.ktor.client.statement.bodyAsChannel
import io.ktor.utils.io.readAvailable
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
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
data class TransferProgress(
    val direction: Direction,
    val fileName: String,
    val transferredBytes: Long,
    val totalBytes: Long?,
) {
    enum class Direction { DOWNLOAD, UPLOAD }
}

class DriveTransfer private constructor(
    private val client: HttpClient,
    private val onProgress: (TransferProgress) -> Unit,
    private val downloadFilesUrl: String,
    private val uploadFilesUrl: String,
) {

    constructor(
        client: HttpClient,
        onProgress: (TransferProgress) -> Unit = {},
    ) : this(client, onProgress, DOWNLOAD_URL, UPLOAD_URL)

    internal constructor(
        client: HttpClient,
        downloadFilesUrl: String,
    ) : this(client, {}, downloadFilesUrl, UPLOAD_URL)

    internal constructor(
        client: HttpClient,
        downloadFilesUrl: String,
        uploadFilesUrl: String,
    ) : this(client, {}, downloadFilesUrl, uploadFilesUrl)

    companion object {
        private const val DOWNLOAD_URL = "https://www.googleapis.com/drive/v3/files"
        private const val UPLOAD_URL = "https://www.googleapis.com/upload/drive/v3/files"
        private const val UPLOAD_CHUNK_SIZE = 8 * 1024 * 1024
        private const val UPLOAD_FIELDS = "id, name, modifiedTime"
        private const val MAX_UPLOAD_RECOVERIES = 3
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
        val response = client.get("$downloadFilesUrl/$fileId") {
            header(HttpHeaders.Authorization, "Bearer $accessToken")
            parameter("alt", "media")
            transferTimeout()
        }

        if (response.status != HttpStatusCode.OK) {
            val errorBody = response.body<String>()
            throw DriveApiException("Drive download failed (${response.status}): $errorBody")
        }

        val channel = response.bodyAsChannel()
        val totalBytes = response.headers[HttpHeaders.ContentLength]?.toLongOrNull()
        try {
            withContext(Dispatchers.IO) {
                val parent = destination.absoluteFile.parentFile
                    ?: throw DriveApiException("Could not resolve destination directory for $destination")
                if (!parent.exists() && !parent.mkdirs()) {
                    throw DriveApiException("Could not create destination directory: $parent")
                }

                var temporaryFile: java.nio.file.Path? = null
                try {
                    temporaryFile = Files.createTempFile(parent.toPath(), ".${destination.name}.", ".part")
                    Files.newOutputStream(temporaryFile).use { output ->
                        val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
                        var transferred = 0L
                        while (!channel.isClosedForRead) {
                            currentCoroutineContext().ensureActive()
                            val count = channel.readAvailable(buffer)
                            if (count == -1) break
                            if (count == 0) continue
                            output.write(buffer, 0, count)
                            transferred += count
                            onProgress(TransferProgress(TransferProgress.Direction.DOWNLOAD, destination.name, transferred, totalBytes))
                        }
                    }
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
        } finally {
            channel.cancel(CancellationException("Drive response body is no longer needed"))
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
        return uploadResumable(
            accessToken = accessToken,
            localFile = localFile,
            driveName = driveName,
            parentFolderId = parentFolderId,
        )
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
        return uploadResumable(
            accessToken = accessToken,
            localFile = localFile,
            fileId = fileId,
        )
    }

    private suspend fun uploadResumable(
        accessToken: String,
        localFile: File,
        driveName: String = localFile.name,
        parentFolderId: String? = null,
        fileId: String? = null,
    ): DriveUploadResponse {
        var sessionRestarts = 0
        while (true) {
            val sessionUri = startResumableUpload(accessToken, localFile, driveName, parentFolderId, fileId)
            try {
                return uploadResumableContent(accessToken, localFile, sessionUri)
            } catch (e: UploadSessionExpiredException) {
                if (sessionRestarts >= 1) {
                    throw DriveApiException("Drive upload session expired after restarting once", e)
                }
                sessionRestarts++
            }
        }
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

        val requestUrl = if (fileId == null) uploadFilesUrl else "$uploadFilesUrl/$fileId"
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
        var recoveryAttempts = 0

        RandomAccessFile(localFile, "r").use { source ->
            while (offset < totalBytes || (totalBytes == 0L && offset == 0L)) {
                val chunkLength = minOf(UPLOAD_CHUNK_SIZE.toLong(), totalBytes - offset).toInt()
                val chunk = ByteArray(chunkLength)
                if (chunkLength > 0) {
                    source.seek(offset)
                    source.readFully(chunk)
                }

                val response = try {
                    client.put(sessionUri) {
                        header(HttpHeaders.Authorization, "Bearer $accessToken")
                        header(HttpHeaders.ContentLength, chunkLength.toString())
                        if (totalBytes > 0) {
                            header(HttpHeaders.ContentRange, "bytes $offset-${offset + chunkLength - 1}/$totalBytes")
                        }
                        contentType(ContentType.Application.OctetStream)
                        setBody(chunk)
                        transferTimeout()
                    }
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Exception) {
                    val progress = queryUploadProgress(accessToken, sessionUri, totalBytes)
                    when (progress) {
                        is UploadProgress.Complete -> return@withContext progress.response
                        is UploadProgress.Incomplete -> {
                            if (progress.nextOffset != offset) {
                                offset = progress.nextOffset
                                recoveryAttempts = 0
                            } else {
                                recoveryAttempts = retryAfterNoProgress(recoveryAttempts, e)
                            }
                            continue
                        }
                    }
                }

                when {
                    response.status == HttpStatusCode.OK || response.status == HttpStatusCode.Created -> {
                        onProgress(TransferProgress(TransferProgress.Direction.UPLOAD, localFile.name, totalBytes, totalBytes))
                        return@withContext response.body<DriveUploadResponse>()
                    }
                    response.status.value == 308 -> {
                        val nextOffset = nextUploadOffset(response.headers[HttpHeaders.Range], totalBytes)
                        if (nextOffset == offset) {
                            recoveryAttempts = retryAfterNoProgress(recoveryAttempts)
                        } else {
                            offset = nextOffset
                            recoveryAttempts = 0
                            onProgress(TransferProgress(TransferProgress.Direction.UPLOAD, localFile.name, offset, totalBytes))
                        }
                    }
                    response.status == HttpStatusCode.NotFound -> throw UploadSessionExpiredException()
                    response.status.value in 500..599 -> {
                        val progress = queryUploadProgress(accessToken, sessionUri, totalBytes)
                        when (progress) {
                            is UploadProgress.Complete -> return@withContext progress.response
                            is UploadProgress.Incomplete -> {
                                if (progress.nextOffset != offset) {
                                    offset = progress.nextOffset
                                    recoveryAttempts = 0
                                } else {
                                    recoveryAttempts = retryAfterNoProgress(recoveryAttempts)
                                }
                            }
                        }
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

    private sealed interface UploadProgress {
        data class Incomplete(val nextOffset: Long) : UploadProgress
        data class Complete(val response: DriveUploadResponse) : UploadProgress
    }

    private class UploadSessionExpiredException : Exception("Drive upload session expired")

    private suspend fun queryUploadProgress(
        accessToken: String,
        sessionUri: String,
        totalBytes: Long,
    ): UploadProgress {
        var lastError: Exception? = null
        repeat(MAX_UPLOAD_RECOVERIES) { attempt ->
            val response = try {
                client.put(sessionUri) {
                    header(HttpHeaders.Authorization, "Bearer $accessToken")
                    header(HttpHeaders.ContentLength, "0")
                    header(HttpHeaders.ContentRange, "bytes */$totalBytes")
                    setBody(ByteArray(0))
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                lastError = e
                delay(retryDelayMs(attempt))
                return@repeat
            }

            when {
                response.status == HttpStatusCode.OK || response.status == HttpStatusCode.Created -> {
                    return UploadProgress.Complete(response.body<DriveUploadResponse>())
                }
                response.status.value == 308 -> {
                    return UploadProgress.Incomplete(nextUploadOffset(response.headers[HttpHeaders.Range], totalBytes))
                }
                response.status == HttpStatusCode.NotFound -> throw UploadSessionExpiredException()
                response.status.value in 500..599 -> {
                    lastError = DriveApiException("Drive upload status check failed (${response.status})")
                    delay(retryDelayMs(attempt))
                }
                else -> {
                    val errorBody = response.body<String>()
                    throw DriveApiException("Drive upload status check failed (${response.status}): $errorBody")
                }
            }
        }

        throw DriveApiException("Could not recover Drive upload status after retries", lastError)
    }

    private fun nextUploadOffset(rangeHeader: String?, totalBytes: Long): Long {
        if (rangeHeader.isNullOrBlank()) return 0L
        val receivedThrough = rangeHeader.substringAfterLast('-').toLongOrNull()
            ?: throw DriveApiException("Invalid Drive upload Range header: $rangeHeader")
        val nextOffset = receivedThrough + 1
        if (nextOffset < 0 || nextOffset > totalBytes) {
            throw DriveApiException("Drive upload Range exceeds file size: $rangeHeader")
        }
        return nextOffset
    }

    private suspend fun retryAfterNoProgress(attempts: Int, cause: Exception? = null): Int {
        val nextAttempt = attempts + 1
        if (nextAttempt > MAX_UPLOAD_RECOVERIES) {
            throw DriveApiException("Drive upload made no progress after $MAX_UPLOAD_RECOVERIES retries", cause)
        }
        delay(retryDelayMs(attempts))
        return nextAttempt
    }

    private fun retryDelayMs(attempt: Int): Long = 250L * (1L shl attempt.coerceAtMost(4))

    private fun io.ktor.client.request.HttpRequestBuilder.transferTimeout() {
        timeout {
            requestTimeoutMillis = HttpTimeoutConfig.INFINITE_TIMEOUT_MS
            socketTimeoutMillis = 60_000
        }
    }
}
