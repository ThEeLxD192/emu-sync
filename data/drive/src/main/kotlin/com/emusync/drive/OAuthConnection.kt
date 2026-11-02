package com.emusync.drive

import com.emusync.domain.config.AppConfig
import com.emusync.domain.config.CredentialStore
import com.emusync.domain.config.OAuthCredentials
import com.emusync.domain.operation.recoverOperationFailure

/** Keeps authorization tied to the same connection revision across network and browser waits. */
internal class OAuthConnection(
    private val config: AppConfig,
    private val manager: CredentialStore,
    private val credentials: OAuthCredentials,
    private val auth: DriveAuth,
) {
    private var expected = manager.snapshotCredentials(config)

    init {
        if (expected.credentials != credentials)
            throw DriveApiException(
                "The Google Drive connection changed. Start synchronization again."
            )
    }

    fun requireCurrent() {
        if (manager.snapshotCredentials(config) != expected) changedConnection()
    }

    suspend fun refresh(): String? {
        val refreshToken = credentials.refreshToken?.takeIf { it.isNotBlank() } ?: return null
        return recoverOperationFailure(
            action = {
                val response =
                    auth.refreshAccessToken(
                        credentials.clientId,
                        credentials.clientSecret,
                        refreshToken,
                    )
                requireCurrent()
                response.accessToken
            },
            recover = { failure ->
                requireCurrent()
                if (failure is DriveApiException && failure.errorCode == "invalid_grant") {
                    persist(null)
                    expected =
                        expected.copy(
                            credentials = credentials.copy(refreshToken = null),
                            revision = expected.revision + 1,
                        )
                    null
                } else
                    throw DriveApiException(
                        "Drive offline or unreachable: ${failure.message.orEmpty()}",
                        failure,
                    )
            },
        )
    }

    suspend fun exchange(code: String, redirectUri: String): String {
        val tokens =
            auth.exchangeCodeForTokens(
                credentials.clientId,
                credentials.clientSecret,
                code,
                redirectUri,
            )
        persist(tokens.refreshToken)
        return tokens.accessToken
    }

    private fun persist(refreshToken: String?) {
        if (
            !manager.compareAndSetCredentials(
                expected,
                credentials.copy(refreshToken = refreshToken),
                config,
            )
        )
            changedConnection()
    }

    private fun changedConnection(): Nothing =
        throw DriveApiException(
            "The Google Drive connection changed during authorization. Connect again to continue."
        )
}
