package com.emusync.drive

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.nio.file.Files
import java.time.Instant

internal fun canonicalCloudFiles(files: List<DriveFile>): List<DriveFile> = files.groupBy { it.name }.values.map { candidates ->
    if (candidates.size > 1 && (candidates.any { it.sha256Checksum == null } ||
        candidates.map { it.sha256Checksum?.lowercase() }.distinct().size > 1)) {
        throw SaveSyncException("Several cloud copies of '${candidates.first().name}' contain different or unverified progress. All copies were kept. Resolve the duplicate files in Drive before synchronizing.")
    }
    candidates.maxWith(compareBy<DriveFile> { file -> java.time.Instant.parse(file.modifiedTime) }.thenBy { file -> file.id })
}

/** Chooses from verified content and the last common observation, without trusting clock ordering. */
internal fun planTrackedSync(localSha256: String?, remote: DriveFile?, previous: SyncedSave?): SyncDecision {
    if (localSha256 == null) return if (remote == null) SyncDecision.IN_SYNC else SyncDecision.DOWNLOAD_CLOUD
    if (remote == null) return SyncDecision.UPLOAD_LOCAL
    if (remote.sha256Checksum?.equals(localSha256, ignoreCase = true) == true) return SyncDecision.IN_SYNC
    if (previous == null) return SyncDecision.CONFLICT
    val localChanged = localSha256 != previous.localSha256
    val remoteChanged = remote.id != previous.remoteId || when {
        remote.sha256Checksum != null -> !remote.sha256Checksum.equals(previous.remoteSha256 ?: previous.localSha256, ignoreCase = true)
        remote.version != null && previous.remoteVersion != null -> remote.version != previous.remoteVersion
        else -> Instant.parse(remote.modifiedTime) != Instant.parse(previous.remoteModifiedTime)
    }
    return when {
        localChanged && remoteChanged -> SyncDecision.CONFLICT
        localChanged -> SyncDecision.UPLOAD_LOCAL
        remoteChanged -> SyncDecision.DOWNLOAD_CLOUD
        else -> SyncDecision.IN_SYNC
    }
}

/** Performs transfers from stable snapshots and advances history only after successful operations. */
internal class TrackedFileSync(private val transfer: DriveTransfer, private val history: SyncHistoryStore) {
    suspend fun plan(local: File, remote: DriveFile?, folderId: String, remoteName: String): SyncDecision = withContext(Dispatchers.IO) {
        try {
            val digest = local.takeIf { it.isFile }?.let(::sha256)
            planTrackedSync(digest, remote, history.get(history.key(local, folderId, remoteName)))
        } catch (failure: Exception) {
            if (failure is CancellationException || failure is SaveSyncException) throw failure
            throw SaveSyncException("Could not inspect ${local.name} safely. Retry synchronization before playing.", failure)
        }
    }

    suspend fun remember(local: File, remote: DriveFile, folderId: String, remoteName: String) = withContext(Dispatchers.IO) {
        val digest = sha256(local)
        val key = history.key(local, folderId, remoteName)
        if (planTrackedSync(digest, remote, history.get(key)) != SyncDecision.IN_SYNC) {
            throw SaveSyncException("${local.name} changed during synchronization. Retry before replacing either copy.")
        }
        history.record(key, digest, remote)
    }

    suspend fun download(token: String, remote: DriveFile, local: File, folderId: String, remoteName: String) {
        val expectedLocalHash = withContext(Dispatchers.IO) { local.takeIf { it.isFile }?.let(::sha256) }
        val staging = try { withContext(Dispatchers.IO) {
            Files.createDirectories(local.absoluteFile.parentFile.toPath())
            Files.createTempFile(local.absoluteFile.parentFile.toPath(), ".${local.name}.", ".restore")
        } } catch (failure: Exception) {
            if (failure is CancellationException) throw failure
            throw SaveSyncException("Could not prepare a safe restore for ${local.name}.", failure)
        }
        try {
            transfer.download(token, remote.id, staging.toFile(), remote.modifiedTime, progressFileName = local.name)
            withContext(Dispatchers.IO) {
                val digest = sha256(staging.toFile())
                if (remote.sha256Checksum != null && !remote.sha256Checksum.equals(digest, ignoreCase = true)) {
                    throw SaveSyncException("Cloud content changed while downloading ${local.name}. The local save was not replaced.")
                }
                if (local.takeIf { it.isFile }?.let(::sha256) != expectedLocalHash) {
                    throw SaveSyncException("${local.name} changed while downloading. Its new local progress was kept.")
                }
                val key = history.key(local, folderId, remoteName)
                if (expectedLocalHash != null && expectedLocalHash != digest) history.backup(key, local, expectedLocalHash)
                moveReplacing(staging, local.toPath())
                history.record(key, digest, remote)
            }
        } catch (failure: Exception) {
            if (failure is CancellationException || failure is SaveSyncException) throw failure
            throw SaveSyncException("Could not restore ${local.name}. Retry synchronization before playing.", failure)
        } finally { withContext(kotlinx.coroutines.NonCancellable + Dispatchers.IO) { Files.deleteIfExists(staging) } }
    }

    suspend fun upload(token: String, local: File, remote: DriveFile?, folderId: String, remoteName: String) {
        val directory = withContext(Dispatchers.IO) { Files.createTempDirectory("emusync-save-upload-") }
        val snapshot = directory.resolve(local.name).toFile()
        try {
            val digest = withContext(Dispatchers.IO) {
                Files.copy(local.toPath(), snapshot.toPath())
                sha256(snapshot).also {
                    if (sha256(local) != it) throw SaveSyncException("${local.name} changed while preparing its upload. Retry synchronization.")
                }
            }
            val result = if (remote == null) transfer.upload(token, snapshot, remoteName, folderId)
                else transfer.updateIfUnchanged(token, remote, snapshot, remoteName)
            withContext(Dispatchers.IO) {
                if (result.sha256Checksum != null && !result.sha256Checksum.equals(digest, ignoreCase = true)) {
                    throw SaveSyncException("The uploaded content of ${local.name} could not be verified. Local progress was kept.")
                }
                val uploaded = DriveFile(result.id, remoteName, result.modifiedTime, sha256Checksum = result.sha256Checksum, version = result.version)
                history.record(history.key(local, folderId, remoteName), digest, uploaded)
                if (sha256(local) != digest) throw SaveSyncException("${local.name} changed during upload. Its newer local progress still needs synchronization.")
                local.setLastModified(Instant.parse(result.modifiedTime).toEpochMilli())
            }
        } finally {
            withContext(kotlinx.coroutines.NonCancellable + Dispatchers.IO) {
                Files.deleteIfExists(snapshot.toPath())
                Files.deleteIfExists(directory)
            }
        }
    }
}
