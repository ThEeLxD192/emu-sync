package com.emusync.domain.sync

/** A named remote object, independent of provider DTOs. */
data class CloudFile(
    val id: String,
    val name: String,
    val modifiedTime: String,
    val isFolder: Boolean = false,
    val sha256Checksum: String? = null,
    val version: String? = null,
) {
    fun toSyncState() = CloudSaveState(id, modifiedTime, sha256Checksum, version)
}
