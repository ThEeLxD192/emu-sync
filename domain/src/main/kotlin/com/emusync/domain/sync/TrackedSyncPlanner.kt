package com.emusync.domain.sync

import java.time.Instant

/**
 * Chooses from verified content and the last common observation, without trusting clock ordering.
 */
fun planTrackedSync(
    localSha256: String?,
    remote: CloudSaveState?,
    previous: SyncCheckpoint?,
): SyncDecision {
    return when {
        localSha256 == null ->
            if (remote == null) SyncDecision.IN_SYNC else SyncDecision.DOWNLOAD_CLOUD
        remote == null -> SyncDecision.UPLOAD_LOCAL
        remote.sha256Checksum?.equals(localSha256, ignoreCase = true) == true ->
            SyncDecision.IN_SYNC
        previous == null -> SyncDecision.CONFLICT
        else ->
            changedSaveDecision(
                localSha256 != previous.localSha256,
                remoteChanged(remote, previous),
            )
    }
}

private fun changedSaveDecision(localChanged: Boolean, remoteChanged: Boolean): SyncDecision =
    when {
        localChanged && remoteChanged -> SyncDecision.CONFLICT
        localChanged -> SyncDecision.UPLOAD_LOCAL
        remoteChanged -> SyncDecision.DOWNLOAD_CLOUD
        else -> SyncDecision.IN_SYNC
    }

private fun remoteChanged(remote: CloudSaveState, previous: SyncCheckpoint): Boolean =
    remote.id != previous.remoteId ||
        when {
            remote.sha256Checksum != null ->
                !remote.sha256Checksum.equals(
                    previous.remoteSha256 ?: previous.localSha256,
                    ignoreCase = true,
                )
            remote.version != null && previous.remoteVersion != null ->
                remote.version != previous.remoteVersion
            else -> Instant.parse(remote.modifiedTime) != Instant.parse(previous.remoteModifiedTime)
        }
