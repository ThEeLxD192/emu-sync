package com.emusync.drive

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** A single file result from the Google Drive Files.list API. */
@Serializable
data class DriveFile(
    val id: String,
    val name: String,
    @SerialName("modifiedTime")
    val modifiedTime: String, // ISO 8601 e.g. "2024-01-15T10:30:00.000Z"
    val mimeType: String? = null,
    val sha256Checksum: String? = null,
    val version: String? = null,
) {
    val isFolder: Boolean
        get() = mimeType == "application/vnd.google-apps.folder"
}
