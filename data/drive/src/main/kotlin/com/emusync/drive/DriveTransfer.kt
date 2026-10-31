package com.emusync.drive

import com.emusync.domain.operation.recoverOperationFailure
import com.emusync.domain.sync.CloudSaveState
import com.emusync.domain.sync.CloudSaveTransfer
import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.get
import io.ktor.client.request.header
import io.ktor.client.request.parameter
import io.ktor.client.statement.bodyAsChannel
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.utils.io.readAvailable
import java.io.File
import java.nio.file.AtomicMoveNotSupportedException
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.time.Instant
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext

/**
 * Handles file download from and upload to Google Drive.
 *
 * - Download: GET file content by ID → write bytes to local file.
 * - Upload (create): POST new file to Drive.
 * - Upload (update): PATCH existing file on Drive.
 */
class DriveTransfer
private constructor(
    private val client: HttpClient,
    private val onProgress: (TransferProgress) -> Unit,
    private val downloadFilesUrl: String,
    uploadFilesUrl: String,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
) : CloudSaveTransfer {

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

    private val resumable = ResumableDriveUpload(client, onProgress, uploadFilesUrl, ioDispatcher)

    internal constructor(
        client: HttpClient,
        ioDispatcher: CoroutineDispatcher,
    ) : this(client, {}, DOWNLOAD_URL, UPLOAD_URL, ioDispatcher)

    companion object {
        private const val DOWNLOAD_URL = "https://www.googleapis.com/drive/v3/files"
        private const val UPLOAD_URL = "https://www.googleapis.com/upload/drive/v3/files"
    }

    override suspend fun downloadSave(
        accessToken: String,
        fileId: String,
        destinationPath: String,
        modifiedTime: String?,
        progressFileName: String,
    ) = download(accessToken, fileId, File(destinationPath), modifiedTime, progressFileName)

    override suspend fun uploadSave(
        accessToken: String,
        sourcePath: String,
        remoteName: String,
        parentFolderId: String,
    ): CloudSaveState =
        upload(accessToken, File(sourcePath), remoteName, parentFolderId).toSyncState()

    override suspend fun replaceSave(
        accessToken: String,
        observed: CloudSaveState,
        sourcePath: String,
        remoteName: String,
    ): CloudSaveState =
        updateIfUnchanged(
                accessToken,
                DriveFile(
                    observed.id,
                    remoteName,
                    observed.modifiedTime,
                    sha256Checksum = observed.sha256Checksum,
                    version = observed.version,
                ),
                File(sourcePath),
                remoteName,
            )
            .toSyncState()

    /**
     * Downloads a file from Google Drive and saves it to a local path.
     *
     * @param accessToken Valid OAuth 2.0 access token.
     * @param fileId Google Drive file ID to download.
     * @param destination Local [File] path where the content will be written.
     * @param modifiedTime Optional ISO 8601 modifiedTime from Drive to synchronize local timestamp.
     * @param progressFileName Display name used for transfer progress notifications.
     * @throws DriveApiException if the download fails.
     */
    internal suspend fun download(
        accessToken: String,
        fileId: String,
        destination: File,
        modifiedTime: String? = null,
        progressFileName: String = destination.name,
    ) {
        val response =
            client.get("$downloadFilesUrl/$fileId") {
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
            withContext(ioDispatcher) {
                val parent = downloadParent(destination)

                var temporaryFile: java.nio.file.Path? = null
                try {
                    temporaryFile =
                        Files.createTempFile(parent.toPath(), ".${destination.name}.", ".part")
                    Files.newOutputStream(temporaryFile).use { output ->
                        copyDownload(channel, output, progressFileName, totalBytes)
                    }
                    try {
                        Files.move(
                            temporaryFile,
                            destination.toPath(),
                            StandardCopyOption.ATOMIC_MOVE,
                            StandardCopyOption.REPLACE_EXISTING,
                        )
                    } catch (_: AtomicMoveNotSupportedException) {
                        Files.move(
                            temporaryFile,
                            destination.toPath(),
                            StandardCopyOption.REPLACE_EXISTING,
                        )
                    }
                    temporaryFile = null

                    if (modifiedTime != null) {
                        recoverOperationFailure(
                            action = {
                                destination.setLastModified(
                                    Instant.parse(modifiedTime).toEpochMilli()
                                )
                            },
                            recover = { _ ->
                                // Ignore timestamp setting failure
                            },
                        )
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
     * @param accessToken Valid OAuth 2.0 access token.
     * @param localFile Local [File] to upload.
     * @param driveName Name for the file on Drive (defaults to the local filename).
     * @param parentFolderId Optional folder ID to upload into (e.g. the entry's subfolder).
     * @return [DriveUploadResponse] with the new file's ID and metadata.
     * @throws DriveApiException if the upload fails.
     */
    internal suspend fun upload(
        accessToken: String,
        localFile: File,
        driveName: String = localFile.name,
        parentFolderId: String? = null,
    ): DriveUploadResponse {
        return resumable.upload(
            accessToken = accessToken,
            localFile = localFile,
            driveName = driveName,
            parentFolderId = parentFolderId,
        )
    }

    internal suspend fun updateIfUnchanged(
        accessToken: String,
        observed: DriveFile,
        localFile: File,
        remoteName: String,
    ): DriveUploadResponse =
        DriveConditionalUpdate(client, onProgress, ioDispatcher = ioDispatcher)
            .update(accessToken, observed, localFile, remoteName)

    /**
     * Updates an existing file on Google Drive with new content.
     *
     * @param accessToken Valid OAuth 2.0 access token.
     * @param fileId ID of the existing file on Drive to update.
     * @param localFile Local [File] with the new content.
     * @return [DriveUploadResponse] with updated metadata.
     * @throws DriveApiException if the update fails.
     */
    internal suspend fun update(
        accessToken: String,
        fileId: String,
        localFile: File,
    ): DriveUploadResponse {
        return resumable.upload(
            accessToken = accessToken,
            localFile = localFile,
            fileId = fileId,
        )
    }

    private fun downloadParent(destination: File): File {
        val parent =
            destination.absoluteFile.parentFile
                ?: throw DriveApiException(
                    "Could not resolve destination directory for $destination"
                )
        if (!parent.exists() && !parent.mkdirs())
            throw DriveApiException("Could not create destination directory: $parent")
        return parent
    }

    private suspend fun copyDownload(
        channel: io.ktor.utils.io.ByteReadChannel,
        output: java.io.OutputStream,
        fileName: String,
        totalBytes: Long?,
    ) {
        val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
        var transferred = 0L
        while (!channel.isClosedForRead) {
            currentCoroutineContext().ensureActive()
            val count = channel.readAvailable(buffer)
            if (count == -1) break
            if (count > 0) {
                output.write(buffer, 0, count)
                transferred += count
                onProgress(
                    TransferProgress(
                        TransferDirection.DOWNLOAD,
                        fileName,
                        transferred,
                        totalBytes,
                    )
                )
            }
        }
    }
}
