package com.emusync.model

import kotlinx.serialization.Serializable

/**
 * Root object deserialized from `config.json`.
 *
 * @property googleDrive Optional Google Drive credentials for cloud sync.
 * @property entries     List of game entries (emulators and native games).
 */
@Serializable
data class AppConfig(
    val googleDrive: GoogleDriveConfig? = null,
    val entries: List<GameEntry> = emptyList(),
)

/**
 * Google Drive OAuth 2.0 credentials stored locally.
 *
 * On first run, [refreshToken] will be `null`. The OAuth loopback flow
 * will obtain one and save it back to `config.json`.
 */
@Serializable
data class GoogleDriveConfig(
    val clientId: String,
    val clientSecret: String,
    val refreshToken: String? = null,
)

