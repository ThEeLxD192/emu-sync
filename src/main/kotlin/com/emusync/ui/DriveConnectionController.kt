package com.emusync.ui

import com.emusync.config.ConfigManager
import com.emusync.domain.config.AppConfig
import com.emusync.domain.config.OAuthCredentials
import com.emusync.domain.library.GameEntry
import com.emusync.drive.OAuthFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update

/** Keeps credential revisions, persisted connection state and visible sync indicators together. */
internal class DriveConnectionController(
    private val state: MutableStateFlow<AppUiState>,
    private val configManager: ConfigManager,
    private val oauthFlow: OAuthFlow,
    private val checkSync: suspend (GameEntry) -> Unit,
) {
    suspend fun setup(clientId: String, clientSecret: String, connectNow: Boolean): String {
        if (state.value.config == null) return "No config loaded"
        val trimmedId = clientId.trim()
        val trimmedSecret = clientSecret.trim()
        return if (trimmedId.isBlank() || trimmedSecret.isBlank()) {
            "Client ID and Client Secret cannot be blank."
        } else {
            val updated = configManager.updateCredentials { current ->
                OAuthCredentials(
                    trimmedId,
                    trimmedSecret,
                    current?.refreshToken?.takeIf {
                        current.clientId == trimmedId && current.clientSecret == trimmedSecret
                    },
                )
            }
            publishConnection(updated)
            if (connectNow) login() else "Google Drive credentials saved."
        }
    }

    fun unlink(): String {
        val config = state.value.config ?: return "No config loaded"
        return if (config.cloudCredentials == null) "Google Drive is not configured."
        else {
            val updated = configManager.updateCredentials { it?.copy(refreshToken = null) }
            publishConnection(updated)
            "Google Drive disconnected."
        }
    }

    suspend fun login(): String {
        val config = state.value.config ?: return "No config loaded"
        return if (config.cloudCredentials == null)
            "Missing googleDrive configuration in config.json"
        else
            recoverUiFailure(
                action = {
                    oauthFlow.authorize(config, allowInteractive = true)
                    publishConnection(configManager.load())
                    state.value.selectedEntry?.let { checkSync(it) }
                    "Google Drive successfully connected!"
                },
                recover = { failure ->
                    refreshStoredConnection()
                    "Google Drive login failed: ${failure.message}"
                },
            )
    }

    fun refreshStoredConnection() {
        val current = configManager.load()
        if (current.cloudCredentials != state.value.config?.cloudCredentials)
            publishConnection(current)
    }

    private fun publishConnection(config: AppConfig) {
        val status =
            if (config.cloudCredentials?.refreshToken.isNullOrBlank())
                CloudSyncStatus.NOT_CONFIGURED
            else CloudSyncStatus.IDLE
        state.update {
            it.copy(
                config = config,
                entrySyncStatus = config.entries.associate { entry -> entry.name to status },
            )
        }
    }
}
