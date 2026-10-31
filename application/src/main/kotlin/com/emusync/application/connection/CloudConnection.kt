package com.emusync.application.connection

import com.emusync.domain.config.AppConfig
import com.emusync.domain.config.CloudAuthorization
import com.emusync.domain.config.ConfigurationStore
import com.emusync.domain.config.OAuthCredentials
import com.emusync.domain.operation.recoverOperationFailure

/** Credential persistence and authorization use the same revision-aware store. */
class CloudConnection(
    private val store: ConfigurationStore,
    private val authorization: CloudAuthorization,
) {
    fun setup(config: AppConfig?, clientId: String, clientSecret: String): ConnectionResult {
        if (config == null) return ConnectionResult(null, "No config loaded")
        val id = clientId.trim()
        val secret = clientSecret.trim()
        return if (id.isBlank() || secret.isBlank())
            ConnectionResult(null, "Client ID and Client Secret cannot be blank.")
        else {
            val updated = store.updateCredentials { current ->
                OAuthCredentials(
                    id,
                    secret,
                    current?.refreshToken?.takeIf {
                        current.clientId == id && current.clientSecret == secret
                    },
                )
            }
            ConnectionResult(updated, "Google Drive credentials saved.")
        }
    }

    fun unlink(config: AppConfig?): ConnectionResult =
        when {
            config == null -> ConnectionResult(null, "No config loaded")
            config.cloudCredentials == null ->
                ConnectionResult(null, "Google Drive is not configured.")
            else ->
                ConnectionResult(
                    store.updateCredentials { it?.copy(refreshToken = null) },
                    "Google Drive disconnected.",
                )
        }

    suspend fun login(config: AppConfig?): ConnectionResult =
        when {
            config == null -> ConnectionResult(null, "No config loaded")
            config.cloudCredentials == null ->
                ConnectionResult(null, "Missing googleDrive configuration in config.json")
            else ->
                recoverOperationFailure(
                    action = {
                        authorization.authorize(config, true)
                        ConnectionResult(
                            store.loadConfig(),
                            "Google Drive successfully connected!",
                            authorized = true,
                        )
                    },
                    recover = { failure ->
                        ConnectionResult(
                            store.loadConfig(),
                            "Google Drive login failed: ${failure.message}",
                        )
                    },
                )
        }

    fun loadStoredConnection() = store.loadConfig()
}
