package com.emusync.config.dto

import kotlinx.serialization.Serializable

/**
 * Google Drive OAuth 2.0 credentials stored locally.
 *
 * On first run, [refreshToken] will be `null`. The OAuth loopback flow will obtain one and save it
 * back to `config.json`.
 */
@Serializable
internal data class GoogleDriveConfigDto(
    val clientId: String,
    val clientSecret: String,
    val refreshToken: String? = null,
)
