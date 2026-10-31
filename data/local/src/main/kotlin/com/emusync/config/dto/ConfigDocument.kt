package com.emusync.config.dto

import kotlinx.serialization.Serializable

@Serializable
internal data class ConfigDocument(
    val googleDrive: GoogleDriveConfigDto? = null,
    val entries: List<GameEntryDto> = emptyList(),
)
