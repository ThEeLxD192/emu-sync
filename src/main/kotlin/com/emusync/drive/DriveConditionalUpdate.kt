package com.emusync.drive

import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.plugins.HttpTimeoutConfig
import io.ktor.client.plugins.timeout
import io.ktor.client.request.get
import io.ktor.client.request.put
import io.ktor.client.request.header
import io.ktor.client.request.parameter
import io.ktor.client.request.setBody
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.content.OutgoingContent
import io.ktor.utils.io.ByteWriteChannel
import io.ktor.utils.io.writeFully
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import java.io.File

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
 * File metadata: https://developers.google.com/workspace/drive/api/reference/rest/v2/files
 * This streams the body without buffering the entire snapshot. Failed updates restart on explicit retry.
 */
internal class DriveConditionalUpdate(
    private val client: HttpClient,
    private val onProgress: (TransferProgress) -> Unit = {},
    private val filesUrl: String = "https://www.googleapis.com/drive/v2/files",
    private val uploadUrl: String = "https://www.googleapis.com/upload/drive/v2/files",
) {
    suspend fun update(token: String, observed: DriveFile, snapshot: File, remoteName: String): DriveUploadResponse {
        val response = client.get("$filesUrl/${observed.id}") {
            header(HttpHeaders.Authorization, "Bearer $token")
            parameter("fields", FIELDS)
        }
        if (response.status == HttpStatusCode.NotFound) throw changed(remoteName)
        if (response.status != HttpStatusCode.OK) throw DriveApiException("Could not verify cloud save (${response.status})")
        val current = try { response.body<ConditionalDriveFile>() } catch (failure: Exception) {
            if (failure is CancellationException) throw failure
            throw SaveSyncException("Could not verify the cloud version of $remoteName. No cloud save was overwritten.", failure)
        }
        val unchanged = when {
            observed.version != null && current.version != null -> observed.version == current.version
            observed.sha256Checksum != null && current.sha256Checksum != null -> observed.sha256Checksum.equals(current.sha256Checksum, true)
            else -> false
        }
        val contentChanged = observed.sha256Checksum != null && current.sha256Checksum != null &&
            !observed.sha256Checksum.equals(current.sha256Checksum, true)
        if (!unchanged || contentChanged || current.id != observed.id || current.title != remoteName) throw changed(remoteName)
        val etag = current.etag
        if (etag == null || !etag.startsWith('"') || !etag.endsWith('"') || etag.length < 3 || '\r' in etag || '\n' in etag) {
            throw SaveSyncException("Drive did not provide a strong version token for $remoteName. No cloud save was overwritten.")
        }
        val length = snapshot.length()
        // A single conditional request avoids a separate check/commit window in a resumable session.
        val uploaded = client.put("$uploadUrl/${observed.id}") {
            header(HttpHeaders.Authorization, "Bearer $token")
            header(HttpHeaders.IfMatch, etag)
            parameter("uploadType", "media")
            parameter("fields", FIELDS)
            setBody(object : OutgoingContent.WriteChannelContent() {
                override val contentType = ContentType.Application.OctetStream
                override val contentLength = length
                override suspend fun writeTo(channel: ByteWriteChannel) = withContext(Dispatchers.IO) {
                    snapshot.inputStream().buffered().use { input ->
                        val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
                        var sent = 0L
                        while (true) {
                            currentCoroutineContext().ensureActive()
                            val count = input.read(buffer)
                            if (count < 0) break
                            channel.writeFully(buffer, 0, count)
                            sent += count
                            onProgress(TransferProgress(TransferProgress.Direction.UPLOAD, snapshot.name, sent, length))
                        }
                    }
                }
            })
            timeout { requestTimeoutMillis = HttpTimeoutConfig.INFINITE_TIMEOUT_MS; socketTimeoutMillis = 60_000 }
        }
        if (uploaded.status == HttpStatusCode.PreconditionFailed || uploaded.status == HttpStatusCode.NotFound || uploaded.status == HttpStatusCode.Conflict) throw changed(remoteName)
        if (uploaded.status != HttpStatusCode.OK) throw DriveApiException("Conditional cloud upload failed (${uploaded.status}): ${uploaded.body<String>()}")
        val result = uploaded.body<ConditionalDriveFile>()
        if (result.id != observed.id || result.title != remoteName) throw SaveSyncException("Unexpected cloud upload response for $remoteName. Verify both copies before retrying.")
        return DriveUploadResponse(result.id, result.title, result.modifiedDate, result.sha256Checksum, result.version)
    }

    private fun changed(name: String) = SaveSyncException("The cloud save '$name' changed on another device. Both copies were kept; synchronize again to resolve the conflict.")
    companion object { private const val FIELDS = "id,title,modifiedDate,etag,version,sha256Checksum" }
}
