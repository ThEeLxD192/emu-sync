package com.emusync.local.saves

import com.emusync.domain.operation.recoverOperationFailure
import com.emusync.domain.sync.CloudSaveState
import com.emusync.domain.sync.CloudSaveTransfer
import com.emusync.domain.sync.LocalSaveFiles
import com.emusync.domain.sync.SaveSyncException
import com.emusync.domain.sync.SyncDecision
import com.emusync.domain.sync.planTrackedSync
import java.io.File
import java.nio.file.Files
import java.time.Instant
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Performs transfers from stable snapshots and advances history only after successful operations.
 */
class TrackedFileSync(
    private val transfer: CloudSaveTransfer,
    private val history: SyncHistoryStore,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
) : LocalSaveFiles {
    override suspend fun plan(
        localPath: String,
        remote: CloudSaveState?,
        folderId: String,
        remoteName: String,
    ): SyncDecision =
        withContext(ioDispatcher) {
            val local = File(localPath)
            recoverOperationFailure(
                action = {
                    val digest = local.takeIf { it.isFile }?.let(::sha256)
                    planTrackedSync(
                        digest,
                        remote,
                        history.get(history.key(local, folderId, remoteName)),
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

    override suspend fun remember(
        localPath: String,
        remote: CloudSaveState,
        folderId: String,
        remoteName: String,
    ) =
        withContext(ioDispatcher) {
            val local = File(localPath)
            val digest = sha256(local)
            val key = history.key(local, folderId, remoteName)
            if (planTrackedSync(digest, remote, history.get(key)) != SyncDecision.IN_SYNC) {
                throw SaveSyncException(
                    "${local.name} changed during synchronization. Retry before replacing either copy."
                )
            }
            history.record(key, digest, remote)
        }

    override suspend fun download(
        token: String,
        remote: CloudSaveState,
        localPath: String,
        folderId: String,
        remoteName: String,
    ) {
        val local = File(localPath)
        val expectedLocalHash =
            withContext(ioDispatcher) { local.takeIf { it.isFile }?.let(::sha256) }
        val staging = prepareRestore(local)
        try {
            recoverOperationFailure(
                action = {
                    transfer.downloadSave(
                        token,
                        remote.id,
                        staging.toString(),
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

    override suspend fun upload(
        token: String,
        localPath: String,
        remote: CloudSaveState?,
        folderId: String,
        remoteName: String,
    ) {
        val local = File(localPath)
        val directory =
            withContext(ioDispatcher) { Files.createTempDirectory("emusync-save-upload-") }
        val snapshot = directory.resolve(local.name).toFile()
        try {
            val digest = withContext(ioDispatcher) { prepareSnapshot(local, snapshot) }
            val result =
                if (remote == null) transfer.uploadSave(token, snapshot.path, remoteName, folderId)
                else transfer.replaceSave(token, remote, snapshot.path, remoteName)
            withContext(ioDispatcher) {
                verifyUpload(result.sha256Checksum, digest, local)
                history.record(history.key(local, folderId, remoteName), digest, result)
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
        recoverOperationFailure(
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
        remote: CloudSaveState,
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
