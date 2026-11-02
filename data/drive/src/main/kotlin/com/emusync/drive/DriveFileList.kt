package com.emusync.drive

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Google Drive Files.list API response. */
@Serializable
internal data class DriveFileList(
    val files: List<DriveFile> = emptyList(),
    @SerialName("nextPageToken") val nextPageToken: String? = null,
)
