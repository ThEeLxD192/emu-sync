package com.emusync.drive

import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.get
import io.ktor.client.request.header
import io.ktor.client.request.parameter
import io.ktor.client.request.put
import io.ktor.client.request.setBody
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.content.OutgoingContent
import io.ktor.utils.io.ByteWriteChannel
import io.ktor.utils.io.writeFully
import java.io.File
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable

@Serializable
internal data class ConditionalDriveFile(
    val id: String,
    val title: String,
    val modifiedDate: String,
    val etag: String? = null,
    val version: String? = null,
    val sha256Checksum: String? = null,
)

/**
 * Uses v2's explicit file ETag for a single conditional media write; no unconditional fallback.
 * File metadata: https://developers.google.com/workspace/drive/api/reference/rest/v2/files This
 * streams the body without buffering the entire snapshot. Failed updates restart on explicit retry.
 */
internal class DriveConditionalUpdate(
    private val client: HttpClient,
    private val onProgress: (TransferProgress) -> Unit = {},
    private val filesUrl: String = "https://www.googleapis.com/drive/v2/files",
    private val uploadUrl: String = "https://www.googleapis.com/upload/drive/v2/files",
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
) {
    suspend fun update(
        token: String,
        observed: DriveFile,
        snapshot: File,
        remoteName: String,
    ): DriveUploadResponse {
        val current = readCurrent(token, observed.id, remoteName)
        val etag = verifiedEtag(observed, current, remoteName)
        val length = snapshot.length()
        // One conditional media request avoids a separate check/commit window.
        val uploaded =
            client.put("$uploadUrl/${observed.id}") {
                header(HttpHeaders.Authorization, "Bearer $token")
                header(HttpHeaders.IfMatch, etag)
                parameter("uploadType", "media")
                parameter("fields", FIELDS)
                setBody(uploadContent(snapshot, length))
                transferTimeout()
            }
        validateUploadStatus(uploaded.status, remoteName, uploaded)
        val result = uploaded.body<ConditionalDriveFile>()
        if (result.id != observed.id || result.title != remoteName)
            throw SaveSyncException(
                "Unexpected cloud upload response for $remoteName. Verify both copies before retrying."
            )
        return DriveUploadResponse(
            result.id,
            result.title,
            result.modifiedDate,
            result.sha256Checksum,
            result.version,
        )
    }

    private suspend fun readCurrent(
        token: String,
        id: String,
        remoteName: String,
    ): ConditionalDriveFile {
        val response =
            client.get("$filesUrl/$id") {
                header(HttpHeaders.Authorization, "Bearer $token")
                parameter("fields", FIELDS)
            }
        validateReadStatus(response.status, remoteName)
        return recoverDriveFailure(
            action = { response.body<ConditionalDriveFile>() },
            recover = { failure ->
                throw SaveSyncException(
                    "Could not verify the cloud version of $remoteName. No cloud save was overwritten.",
                    failure,
                )
            },
        )
    }

    private fun validateReadStatus(status: HttpStatusCode, remoteName: String) {
        if (status == HttpStatusCode.NotFound) throw changed(remoteName)
        if (status != HttpStatusCode.OK)
            throw DriveApiException("Could not verify cloud save ($status)")
    }

    private fun verifiedEtag(
        observed: DriveFile,
        current: ConditionalDriveFile,
        remoteName: String,
    ): String {
        val unchanged =
            when {
                observed.version != null && current.version != null ->
                    observed.version == current.version
                observed.sha256Checksum != null && current.sha256Checksum != null ->
                    observed.sha256Checksum.equals(current.sha256Checksum, true)
                else -> false
            }
        val contentChanged =
            observed.sha256Checksum != null &&
                current.sha256Checksum != null &&
                !observed.sha256Checksum.equals(current.sha256Checksum, true)
        val identityChanged = current.id != observed.id || current.title != remoteName
        if (!unchanged || contentChanged || identityChanged) throw changed(remoteName)
        return current.etag?.takeIf(::isStrongEtag)
            ?: throw SaveSyncException(
                "Drive did not provide a strong version token for $remoteName. No cloud save was overwritten."
            )
    }

    private fun isStrongEtag(etag: String): Boolean =
        etag.startsWith('"') &&
            etag.endsWith('"') &&
            etag.length >= MIN_ETAG_LENGTH &&
            '\r' !in etag &&
            '\n' !in etag

    private suspend fun validateUploadStatus(
        status: HttpStatusCode,
        remoteName: String,
        response: io.ktor.client.statement.HttpResponse,
    ) {
        if (
            status == HttpStatusCode.PreconditionFailed ||
                status == HttpStatusCode.NotFound ||
                status == HttpStatusCode.Conflict
        )
            throw changed(remoteName)
        if (status != HttpStatusCode.OK)
            throw DriveApiException(
                "Conditional cloud upload failed ($status): ${response.body<String>()}"
            )
    }

    private fun uploadContent(snapshot: File, length: Long): OutgoingContent =
        object : OutgoingContent.WriteChannelContent() {
            override val contentType = ContentType.Application.OctetStream
            override val contentLength = length

            override suspend fun writeTo(channel: ByteWriteChannel) =
                withContext(ioDispatcher) {
                    snapshot.inputStream().buffered().use { input ->
                        val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
                        var sent = 0L
                        while (true) {
                            currentCoroutineContext().ensureActive()
                            val count = input.read(buffer)
                            if (count < 0) break
                            channel.writeFully(buffer, 0, count)
                            sent += count
                            onProgress(
                                TransferProgress(
                                    TransferProgress.Direction.UPLOAD,
                                    snapshot.name,
                                    sent,
                                    length,
                                )
                            )
                        }
                    }
                }
        }

    private fun changed(name: String) =
        SaveSyncException(
            "The cloud save '$name' changed on another device. Both copies were kept; synchronize again to resolve the conflict."
        )

    companion object {
        private const val MIN_ETAG_LENGTH = 3
        private const val FIELDS = "id,title,modifiedDate,etag,version,sha256Checksum"
    }
}
