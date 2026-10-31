package com.emusync.ui

import com.emusync.application.connection.CloudConnection
import com.emusync.config.ConfigManager
import com.emusync.domain.config.AppConfig
import com.emusync.domain.config.CloudAuthorization
import com.emusync.domain.library.GameEntry
import com.emusync.drive.OAuthFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update

/** Keeps credential revisions, persisted connection state and visible sync indicators together. */
internal class DriveConnectionController(
    private val state: MutableStateFlow<AppUiState>,
    configManager: ConfigManager,
    oauthFlow: OAuthFlow,
    private val checkSync: suspend (GameEntry) -> Unit,
) {
    private val connection =
        CloudConnection(
            configManager,
            CloudAuthorization { config, interactive -> oauthFlow.authorize(config, interactive) },
        )

    suspend fun setup(clientId: String, clientSecret: String, connectNow: Boolean): String {
        val result = connection.setup(state.value.config, clientId, clientSecret)
        result.config?.let(::publishConnection)
        return if (result.config != null && connectNow) login() else result.message
    }

    fun unlink(): String {
        val result = connection.unlink(state.value.config)
        result.config?.let(::publishConnection)
        return result.message
    }

    suspend fun login(): String =
        recoverUiFailure(
            action = {
                val result = connection.login(state.value.config)
                if (
                    result.authorized ||
                        result.config?.cloudCredentials != state.value.config?.cloudCredentials
                )
                    result.config?.let(::publishConnection)
                if (result.authorized) state.value.selectedEntry?.let { checkSync(it) }
                result.message
            },
            recover = { failure ->
                refreshStoredConnection()
                "Google Drive login failed: ${failure.message}"
            },
        )

    fun refreshStoredConnection() {
        val current = connection.loadStoredConnection()
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
