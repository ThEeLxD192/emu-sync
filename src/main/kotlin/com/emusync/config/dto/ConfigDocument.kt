package com.emusync.config.dto

import com.emusync.model.GoogleDriveConfig
import kotlinx.serialization.Serializable

@Serializable
internal data class ConfigDocument(
    val googleDrive: GoogleDriveConfig? = null,
    val entries: List<GameEntryDto> = emptyList(),
)
