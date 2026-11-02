package com.emusync.domain.config

import com.emusync.domain.library.GameEntry

/**
 * Application settings; persistence adapters translate their own document format.
 *
 * @property cloudCredentials Optional OAuth credentials for cloud sync.
 * @property entries List of game entries (emulators and native games).
 */
data class AppConfig(
    val cloudCredentials: OAuthCredentials? = null,
    val entries: List<GameEntry> = emptyList(),
)
