package com.emusync.local.saves

import kotlinx.serialization.Serializable

@Serializable
internal data class SyncedSave(
    val localSha256: String,
    val remoteId: String,
    val remoteModifiedTime: String,
    val remoteSha256: String? = null,
    val remoteVersion: String? = null,
)
