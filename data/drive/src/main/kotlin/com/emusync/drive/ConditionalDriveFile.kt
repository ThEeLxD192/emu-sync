package com.emusync.drive

import kotlinx.serialization.Serializable

@Serializable
internal data class ConditionalDriveFile(
    val id: String,
    val title: String,
    val modifiedDate: String,
    val etag: String? = null,
    val version: String? = null,
    val sha256Checksum: String? = null,
)
