package com.emusync.drive

import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.forms.submitForm
import io.ktor.http.HttpStatusCode
import io.ktor.http.parameters
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/**
 * Handles Google OAuth 2.0 authentication.
 *
 * Two flows:
 * 1. **Initial authorization**: [exchangeCodeForTokens] — exchanges an authorization code (from the
 *    loopback redirect) for access + refresh tokens.
 * 2. **Subsequent sessions**: [refreshAccessToken] — uses the stored refresh token to get a new
 *    short-lived access token.
 */
internal class DriveAuth(private val client: HttpClient) {

    companion object {
        private const val TOKEN_URL = "https://oauth2.googleapis.com/token"
    }

    /**
     * Exchanges an authorization code for access and refresh tokens. Called once during the initial
     * OAuth loopback flow.
     *
     * @param clientId OAuth 2.0 client ID.
     * @param clientSecret OAuth 2.0 client secret.
     * @param code Authorization code from the loopback redirect.
     * @param redirectUri Must match the redirect URI used in the authorization request.
     * @return [AuthTokens] containing both access and refresh tokens.
     */
    suspend fun exchangeCodeForTokens(
        clientId: String,
        clientSecret: String,
        code: String,
        redirectUri: String,
    ): AuthTokens {
        val response =
            client.submitForm(
                url = TOKEN_URL,
                formParameters =
                    parameters {
                        append("client_id", clientId)
                        append("client_secret", clientSecret)
                        append("code", code)
                        append("redirect_uri", redirectUri)
                        append("grant_type", "authorization_code")
                    },
            )

        if (response.status != HttpStatusCode.OK) {
            val errorBody = response.body<String>()
            throw DriveApiException("Code exchange failed (${response.status}): $errorBody")
        }

        return response.body<AuthTokens>()
    }

    /**
     * Exchanges a refresh token for a new access token. Used on every session after initial
     * authorization.
     *
     * @param clientId OAuth 2.0 client ID.
     * @param clientSecret OAuth 2.0 client secret.
     * @param refreshToken Stored offline refresh token.
     * @return [TokenResponse] containing the access token and its TTL.
     */
    suspend fun refreshAccessToken(
        clientId: String,
        clientSecret: String,
        refreshToken: String,
    ): TokenResponse {
        val response =
            client.submitForm(
                url = TOKEN_URL,
                formParameters =
                    parameters {
                        append("client_id", clientId)
                        append("client_secret", clientSecret)
                        append("refresh_token", refreshToken)
                        append("grant_type", "refresh_token")
                    },
            )

        if (response.status != HttpStatusCode.OK) {
            val errorBody = response.body<String>()
            val errorCode = runCatching {
                Json.parseToJsonElement(errorBody).jsonObject["error"]?.jsonPrimitive?.content
            }
                .getOrNull()
            throw DriveApiException(
                "Token refresh failed (${response.status}): $errorBody",
                errorCode = errorCode,
            )
        }

        return response.body<TokenResponse>()
    }
}

/** Exception for Google Drive API errors. */
