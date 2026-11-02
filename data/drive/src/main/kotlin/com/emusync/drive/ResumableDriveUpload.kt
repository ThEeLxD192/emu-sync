package com.emusync.drive

import com.emusync.domain.operation.recoverOperationFailure
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
import java.io.File
import java.io.RandomAccessFile
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext

/** Owns resumable-session offsets, recovery and completion verification. */
internal class ResumableDriveUpload(
    private val client: HttpClient,
    private val onProgress: (TransferProgress) -> Unit,
    private val uploadFilesUrl: String,
    private val ioDispatcher: CoroutineDispatcher,
) {
    companion object {
        private const val UPLOAD_FIELDS = "id, name, modifiedTime, sha256Checksum, version"
        private const val MAX_UPLOAD_RECOVERIES = 3
        private const val RESUME_INCOMPLETE_STATUS = 308
        private const val FIRST_SERVER_ERROR = 500
        private const val LAST_SERVER_ERROR = 599
        private const val INITIAL_RETRY_DELAY_MS = 250L
        private const val MAX_RETRY_EXPONENT = 4
    }

    suspend fun upload(
        accessToken: String,
        localFile: File,
        driveName: String = localFile.name,
        parentFolderId: String? = null,
        fileId: String? = null,
    ): DriveUploadResponse {
        var sessionRestarts = 0
        while (true) {
            val sessionUri =
                startResumableUpload(accessToken, localFile, driveName, parentFolderId, fileId)
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
        val metadata =
            if (fileId == null) {
                val escapedName = driveName.replace("\\", "\\\\").replace("\"", "\\\"")
                val parentsJson = (parentFolderId?.let { ", \"parents\": [\"$it\"]" }).orEmpty()
                "{\"name\": \"$escapedName\"$parentsJson}"
            } else {
                "{}"
            }

        val requestUrl = if (fileId == null) uploadFilesUrl else "$uploadFilesUrl/$fileId"
        val response =
            if (fileId == null) {
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
            throw DriveApiException(
                "Drive upload session creation failed (${response.status}): $errorBody"
            )
        }

        return response.headers[HttpHeaders.Location]
            ?: throw DriveApiException(
                "Drive upload session response did not include a Location header"
            )
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
    ): DriveUploadResponse =
        withContext(ioDispatcher) {
            val cursor = UploadCursor(localFile.length())
            RandomAccessFile(localFile, "r").use { source ->
                do {
                    val chunk = readUploadChunk(source, cursor.offset, cursor.totalBytes)
                    when (
                        val progress =
                            uploadChunk(accessToken, sessionUri, chunk, cursor, localFile.name)
                    ) {
                        is UploadProgress.Complete -> return@withContext progress.response
                        is UploadProgress.Incomplete ->
                            advanceUpload(cursor, progress, localFile.name)
                    }
                    if (cursor.totalBytes == 0L) break
                } while (cursor.offset < cursor.totalBytes)
            }
            throw DriveApiException("Drive upload ended without a completed file response")
        }

    private suspend fun uploadChunk(
        token: String,
        sessionUri: String,
        chunk: ByteArray,
        cursor: UploadCursor,
        fileName: String,
    ): UploadProgress {
        val response =
            recoverOperationFailure(
                action = {
                    client.put(sessionUri) {
                        header(HttpHeaders.Authorization, "Bearer $token")
                        header(HttpHeaders.ContentLength, chunk.size.toString())
                        if (cursor.totalBytes > 0)
                            header(
                                HttpHeaders.ContentRange,
                                "bytes ${cursor.offset}-${cursor.offset + chunk.size - 1}/${cursor.totalBytes}",
                            )
                        contentType(ContentType.Application.OctetStream)
                        setBody(chunk)
                        transferTimeout()
                    }
                },
                recover = { failure ->
                    return when (
                        val progress = queryUploadProgress(token, sessionUri, cursor.totalBytes)
                    ) {
                        is UploadProgress.Complete -> progress
                        is UploadProgress.Incomplete -> progress.copy(cause = failure)
                    }
                },
            )
        return decodeProgress(response, cursor.totalBytes, fileName)
            ?: queryUploadProgress(token, sessionUri, cursor.totalBytes)
    }

    private suspend fun advanceUpload(
        cursor: UploadCursor,
        progress: UploadProgress.Incomplete,
        fileName: String,
    ) {
        if (progress.nextOffset != cursor.offset) {
            cursor.offset = progress.nextOffset
            cursor.recoveryAttempts = 0
            if (progress.report)
                onProgress(
                    TransferProgress(
                        TransferDirection.UPLOAD,
                        fileName,
                        cursor.offset,
                        cursor.totalBytes,
                    )
                )
        } else
            cursor.recoveryAttempts = retryAfterNoProgress(cursor.recoveryAttempts, progress.cause)
    }

    private suspend fun decodeProgress(
        response: io.ktor.client.statement.HttpResponse,
        totalBytes: Long,
        fileName: String? = null,
    ): UploadProgress? =
        when {
            response.status == HttpStatusCode.OK || response.status == HttpStatusCode.Created -> {
                if (fileName != null)
                    onProgress(
                        TransferProgress(
                            TransferDirection.UPLOAD,
                            fileName,
                            totalBytes,
                            totalBytes,
                        )
                    )
                UploadProgress.Complete(response.body<DriveUploadResponse>())
            }
            response.status.value == RESUME_INCOMPLETE_STATUS ->
                UploadProgress.Incomplete(
                    nextUploadOffset(response.headers[HttpHeaders.Range], totalBytes),
                    report = fileName != null,
                )
            response.status == HttpStatusCode.NotFound -> throw UploadSessionExpiredException()
            response.status.value in FIRST_SERVER_ERROR..LAST_SERVER_ERROR -> null
            else -> {
                val errorBody = response.body<String>()
                val message =
                    if (fileName == null) "Drive upload status check failed"
                    else "Drive upload failed"
                throw DriveApiException("$message (${response.status}): $errorBody")
            }
        }

    private data class UploadCursor(
        val totalBytes: Long,
        var offset: Long = 0L,
        var recoveryAttempts: Int = 0,
    )

    private sealed interface UploadProgress {
        data class Incomplete(
            val nextOffset: Long,
            val report: Boolean = false,
            val cause: Exception? = null,
        ) : UploadProgress

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
            val response =
                recoverOperationFailure(
                    action = {
                        client.put(sessionUri) {
                            header(HttpHeaders.Authorization, "Bearer $accessToken")
                            header(HttpHeaders.ContentLength, "0")
                            header(HttpHeaders.ContentRange, "bytes */$totalBytes")
                            setBody(ByteArray(0))
                        }
                    },
                    recover = { e ->
                        lastError = e
                        delay(retryDelayMs(attempt))
                        return@repeat
                    },
                )
            decodeProgress(response, totalBytes)?.let {
                return it
            }
            lastError = DriveApiException("Drive upload status check failed (${response.status})")
            delay(retryDelayMs(attempt))
        }
        throw DriveApiException("Could not recover Drive upload status after retries", lastError)
    }

    private fun nextUploadOffset(rangeHeader: String?, totalBytes: Long): Long {
        if (rangeHeader.isNullOrBlank()) return 0L
        val receivedThrough =
            rangeHeader.substringAfterLast('-').toLongOrNull()
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
            throw DriveApiException(
                "Drive upload made no progress after $MAX_UPLOAD_RECOVERIES retries",
                cause,
            )
        }
        delay(retryDelayMs(attempts))
        return nextAttempt
    }

    private fun retryDelayMs(attempt: Int): Long =
        INITIAL_RETRY_DELAY_MS * (1L shl attempt.coerceAtMost(MAX_RETRY_EXPONENT))
}

private const val UPLOAD_CHUNK_SIZE = 8 * 1024 * 1024

private fun readUploadChunk(source: RandomAccessFile, offset: Long, totalBytes: Long): ByteArray {
    val length = minOf(UPLOAD_CHUNK_SIZE.toLong(), totalBytes - offset).toInt()
    return ByteArray(length).also { chunk ->
        if (length > 0) {
            source.seek(offset)
            source.readFully(chunk)
        }
    }
}
