package com.emusync.drive

import com.emusync.domain.sync.SyncDecision
import com.emusync.domain.sync.planTrackedSync
import java.io.File
import java.nio.file.Files
import java.time.Instant
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

internal fun canonicalCloudFiles(files: List<DriveFile>): List<DriveFile> =
    files
        .groupBy { it.name }
        .values
        .map { candidates ->
            if (
                candidates.size > 1 &&
                    (candidates.any { it.sha256Checksum == null } ||
                        candidates.map { it.sha256Checksum?.lowercase() }.distinct().size > 1)
            ) {
                throw SaveSyncException(
                    "Several cloud copies of '${candidates.first().name}' contain different or unverified progress. All copies were kept. Resolve the duplicate files in Drive before synchronizing."
                )
            }
            candidates.maxWith(
                compareBy<DriveFile> { file -> java.time.Instant.parse(file.modifiedTime) }
                    .thenBy { file -> file.id }
            )
        }

/**
 * Performs transfers from stable snapshots and advances history only after successful operations.
 */
internal class TrackedFileSync(
    private val transfer: DriveTransfer,
    private val history: SyncHistoryStore,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
) {
    suspend fun plan(
        local: File,
        remote: DriveFile?,
        folderId: String,
        remoteName: String,
    ): SyncDecision =
        withContext(ioDispatcher) {
            recoverDriveFailure(
                action = {
                    val digest = local.takeIf { it.isFile }?.let(::sha256)
                    planTrackedSync(
                        digest,
                        remote?.toSyncState(),
                        history.get(history.key(local, folderId, remoteName))?.toCheckpoint(),
                    )
                },
                recover = { failure ->
                    if (failure is SaveSyncException) throw failure
                    throw SaveSyncException(
                        "Could not inspect ${local.name} safely. Retry synchronization before playing.",
                        failure,
                    )
                },
            )
        }

    suspend fun remember(local: File, remote: DriveFile, folderId: String, remoteName: String) =
        withContext(ioDispatcher) {
            val digest = sha256(local)
            val key = history.key(local, folderId, remoteName)
            if (
                planTrackedSync(digest, remote.toSyncState(), history.get(key)?.toCheckpoint()) !=
                    SyncDecision.IN_SYNC
            ) {
                throw SaveSyncException(
                    "${local.name} changed during synchronization. Retry before replacing either copy."
                )
            }
            history.record(key, digest, remote)
        }

    suspend fun download(
        token: String,
        remote: DriveFile,
        local: File,
        folderId: String,
        remoteName: String,
    ) {
        val expectedLocalHash =
            withContext(ioDispatcher) { local.takeIf { it.isFile }?.let(::sha256) }
        val staging = prepareRestore(local)
        try {
            recoverDriveFailure(
                action = {
                    transfer.download(
                        token,
                        remote.id,
                        staging.toFile(),
                        remote.modifiedTime,
                        progressFileName = local.name,
                    )
                    withContext(ioDispatcher) {
                        val digest = sha256(staging.toFile())
                        verifyRestore(remote, digest, local, expectedLocalHash)
                        val key = history.key(local, folderId, remoteName)
                        if (expectedLocalHash != null && expectedLocalHash != digest)
                            history.backup(key, local, expectedLocalHash)
                        moveReplacing(staging, local.toPath())
                        history.record(key, digest, remote)
                    }
                },
                recover = { failure ->
                    if (failure is SaveSyncException) throw failure
                    throw SaveSyncException(
                        "Could not restore ${local.name}. Retry synchronization before playing.",
                        failure,
                    )
                },
            )
        } finally {
            withContext(kotlinx.coroutines.NonCancellable + ioDispatcher) {
                Files.deleteIfExists(staging)
            }
        }
    }

    suspend fun upload(
        token: String,
        local: File,
        remote: DriveFile?,
        folderId: String,
        remoteName: String,
    ) {
        val directory =
            withContext(ioDispatcher) { Files.createTempDirectory("emusync-save-upload-") }
        val snapshot = directory.resolve(local.name).toFile()
        try {
            val digest = withContext(ioDispatcher) { prepareSnapshot(local, snapshot) }
            val result =
                if (remote == null) transfer.upload(token, snapshot, remoteName, folderId)
                else transfer.updateIfUnchanged(token, remote, snapshot, remoteName)
            withContext(ioDispatcher) {
                verifyUpload(result.sha256Checksum, digest, local)
                val uploaded =
                    DriveFile(
                        result.id,
                        remoteName,
                        result.modifiedTime,
                        sha256Checksum = result.sha256Checksum,
                        version = result.version,
                    )
                history.record(history.key(local, folderId, remoteName), digest, uploaded)
                if (sha256(local) != digest)
                    throw SaveSyncException(
                        "${local.name} changed during upload. Its newer local progress still needs synchronization."
                    )
                local.setLastModified(Instant.parse(result.modifiedTime).toEpochMilli())
            }
        } finally {
            withContext(kotlinx.coroutines.NonCancellable + ioDispatcher) {
                Files.deleteIfExists(snapshot.toPath())
                Files.deleteIfExists(directory)
            }
        }
    }

    private suspend fun prepareRestore(local: File): java.nio.file.Path =
        recoverDriveFailure(
            action = {
                withContext(ioDispatcher) {
                    Files.createDirectories(local.absoluteFile.parentFile.toPath())
                    Files.createTempFile(
                        local.absoluteFile.parentFile.toPath(),
                        ".${local.name}.",
                        ".restore",
                    )
                }
            },
            recover = { failure ->
                throw SaveSyncException(
                    "Could not prepare a safe restore for ${local.name}.",
                    failure,
                )
            },
        )

    private fun verifyRestore(
        remote: DriveFile,
        digest: String,
        local: File,
        expectedLocalHash: String?,
    ) {
        if (
            remote.sha256Checksum != null &&
                !remote.sha256Checksum.equals(digest, ignoreCase = true)
        ) {
            throw SaveSyncException(
                "Cloud content changed while downloading ${local.name}. The local save was not replaced."
            )
        }
        if (local.takeIf { it.isFile }?.let(::sha256) != expectedLocalHash) {
            throw SaveSyncException(
                "${local.name} changed while downloading. Its new local progress was kept."
            )
        }
    }

    private fun prepareSnapshot(local: File, snapshot: File): String {
        Files.copy(local.toPath(), snapshot.toPath())
        return sha256(snapshot).also {
            if (sha256(local) != it)
                throw SaveSyncException(
                    "${local.name} changed while preparing its upload. Retry synchronization."
                )
        }
    }

    private fun verifyUpload(checksum: String?, digest: String, local: File) {
        if (checksum != null && !checksum.equals(digest, ignoreCase = true)) {
            throw SaveSyncException(
                "The uploaded content of ${local.name} could not be verified. Local progress was kept."
            )
        }
    }
}
