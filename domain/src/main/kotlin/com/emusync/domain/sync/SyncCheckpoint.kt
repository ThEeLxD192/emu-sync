package com.emusync.domain.sync

/** Last common local and remote observations, without storage or wire-format annotations. */
data class SyncCheckpoint(
    val localSha256: String,
    val remoteId: String,
    val remoteModifiedTime: String,
    val remoteSha256: String? = null,
    val remoteVersion: String? = null,
)
