package com.emusync.drive

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Response from Drive upload (create or update). */
@Serializable
internal data class DriveUploadResponse(
    val id: String,
    val name: String,
    @SerialName("modifiedTime") val modifiedTime: String,
    val sha256Checksum: String? = null,
    val version: String? = null,
)
