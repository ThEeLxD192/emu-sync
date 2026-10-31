package com.emusync.drive

import com.emusync.domain.sync.CloudSaveState
import com.emusync.domain.sync.SyncCheckpoint

internal fun DriveFile.toSyncState() = CloudSaveState(id, modifiedTime, sha256Checksum, version)

internal fun SyncedSave.toCheckpoint() =
    SyncCheckpoint(localSha256, remoteId, remoteModifiedTime, remoteSha256, remoteVersion)
