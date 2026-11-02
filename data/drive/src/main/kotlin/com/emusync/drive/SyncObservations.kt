package com.emusync.drive

import com.emusync.domain.sync.CloudSaveState

fun DriveFile.toSyncState() = CloudSaveState(id, modifiedTime, sha256Checksum, version)

internal fun DriveUploadResponse.toSyncState() =
    CloudSaveState(id, modifiedTime, sha256Checksum, version)
