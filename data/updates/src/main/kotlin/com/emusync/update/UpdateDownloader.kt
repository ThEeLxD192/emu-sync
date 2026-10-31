package com.emusync.update

import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.plugins.timeout
import io.ktor.client.request.get
import io.ktor.client.request.header
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.contentLength
import io.ktor.utils.io.ByteReadChannel
import io.ktor.utils.io.readAvailable
import java.io.File
import java.nio.file.Files
import java.nio.file.Path
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext

internal const val UPDATE_SOCKET_TIMEOUT_MS = 60_000L
private const val DOWNLOAD_BUFFER_BYTES = 64 * 1024

/** Publishes a downloaded update only after its complete contents have been staged and checked. */
internal class UpdateDownloader(
    private val client: HttpClient,
    private val ioDispatcher: CoroutineDispatcher,
) {
    // Result is the UI error boundary for transport, storage and progress callback exceptions.
    // Coroutine cancellation must escape rather than become a failed download result.
    @Suppress("TooGenericExceptionCaught")
    suspend fun download(
        downloadUrl: String,
        destination: File,
        onProgress: (Float) -> Unit,
    ): Result<Unit> =
        withContext(ioDispatcher) {
            var staged: Path? = null
            var channel: ByteReadChannel? = null
            try {
                val parent = destination.absoluteFile.parentFile
                Files.createDirectories(parent.toPath())
                staged = Files.createTempFile(parent.toPath(), ".${destination.name}.", ".part")
                val response =
                    client.get(downloadUrl) {
                        header(HttpHeaders.UserAgent, "EmuSync-Desktop-App")
                        timeout {
                            requestTimeoutMillis = null
                            socketTimeoutMillis = UPDATE_SOCKET_TIMEOUT_MS
                        }
                    }
                if (response.status != HttpStatusCode.OK) {
                    return@withContext Result.failure(
                        IllegalStateException(
                            "HTTP ${response.status.value} ${response.status.description}"
                        )
                    )
                }
                val totalBytes = response.contentLength()
                val body = response.body<ByteReadChannel>()
                channel = body
                val bytesRead = writeDownload(body, staged, totalBytes, onProgress)
                validateDownload(bytesRead, totalBytes)
                currentCoroutineContext().ensureActive()
                staged.toFile().setExecutable(true, false)
                moveReplacing(staged, destination.toPath())
                staged = null
                onProgress(1f)
                Result.success(Unit)
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (failure: Exception) {
                Result.failure(failure)
            } finally {
                channel?.cancel(CancellationException("Update response is no longer needed"))
                staged?.let { Files.deleteIfExists(it) }
            }
        }

    private suspend fun writeDownload(
        body: ByteReadChannel,
        staged: Path,
        totalBytes: Long?,
        onProgress: (Float) -> Unit,
    ): Long {
        var bytesRead = 0L
        Files.newOutputStream(staged).use { output ->
            val buffer = ByteArray(DOWNLOAD_BUFFER_BYTES)
            while (!body.isClosedForRead) {
                currentCoroutineContext().ensureActive()
                val read = body.readAvailable(buffer, 0, buffer.size)
                if (read < 0) break
                if (read > 0) {
                    output.write(buffer, 0, read)
                    bytesRead += read
                    if (totalBytes != null && totalBytes > 0) {
                        onProgress((bytesRead.toFloat() / totalBytes).coerceIn(0f, 1f))
                    }
                }
            }
        }
        return bytesRead
    }

    private fun validateDownload(bytesRead: Long, totalBytes: Long?) {
        check(bytesRead > 0) { "The downloaded update is empty" }
        check(totalBytes == null || bytesRead == totalBytes) {
            "The downloaded update is incomplete: expected $totalBytes bytes, received $bytesRead"
        }
    }
}
