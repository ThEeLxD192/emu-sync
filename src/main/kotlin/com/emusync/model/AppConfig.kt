package com.emusync.model

import com.emusync.domain.library.GameEntry

/**
 * Root object deserialized from `config.json`.
 *
 * @property googleDrive Optional Google Drive credentials for cloud sync.
 * @property entries List of game entries (emulators and native games).
 */
data class AppConfig(
    val googleDrive: GoogleDriveConfig? = null,
    val entries: List<GameEntry> = emptyList(),
)
