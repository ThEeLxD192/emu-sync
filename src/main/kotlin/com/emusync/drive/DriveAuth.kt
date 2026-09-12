package com.emusync.drive

import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.forms.submitForm
import io.ktor.http.HttpStatusCode
import io.ktor.http.parameters
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * Token response from the refresh flow (no refresh_token included).
 */
@Serializable
data class TokenResponse(
    @SerialName("access_token") val accessToken: String,
    @SerialName("expires_in") val expiresIn: Int,
    @SerialName("token_type") val tokenType: String,
)

/**
 * Full token response from the initial authorization code exchange.
 * Includes the [refreshToken] which must be persisted to config.json.
 */
@Serializable
data class AuthTokens(
    @SerialName("access_token") val accessToken: String,
    @SerialName("refresh_token") val refreshToken: String,
    @SerialName("expires_in") val expiresIn: Int,
    @SerialName("token_type") val tokenType: String,
)

/**
 * Handles Google OAuth 2.0 authentication.
 *
 * Two flows:
 * 1. **Initial authorization**: [exchangeCodeForTokens] — exchanges an authorization code
 *    (from the loopback redirect) for access + refresh tokens.
 * 2. **Subsequent sessions**: [refreshAccessToken] — uses the stored refresh token
 *    to get a new short-lived access token.
 */
class DriveAuth(private val client: HttpClient) {

    companion object {
        private const val TOKEN_URL = "https://oauth2.googleapis.com/token"
    }

    /**
     * Exchanges an authorization code for access and refresh tokens.
     * Called once during the initial OAuth loopback flow.
     *
     * @param clientId     OAuth 2.0 client ID.
     * @param clientSecret OAuth 2.0 client secret.
     * @param code         Authorization code from the loopback redirect.
     * @param redirectUri  Must match the redirect URI used in the authorization request.
     * @return [AuthTokens] containing both access and refresh tokens.
     */
    suspend fun exchangeCodeForTokens(
        clientId: String,
        clientSecret: String,
        code: String,
        redirectUri: String,
    ): AuthTokens {
        val response = client.submitForm(
            url = TOKEN_URL,
            formParameters = parameters {
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
     * Exchanges a refresh token for a new access token.
     * Used on every session after initial authorization.
     *
     * @param clientId     OAuth 2.0 client ID.
     * @param clientSecret OAuth 2.0 client secret.
     * @param refreshToken Stored offline refresh token.
     * @return [TokenResponse] containing the access token and its TTL.
     */
    suspend fun refreshAccessToken(
        clientId: String,
        clientSecret: String,
        refreshToken: String,
    ): TokenResponse {
        val response = client.submitForm(
            url = TOKEN_URL,
            formParameters = parameters {
                append("client_id", clientId)
                append("client_secret", clientSecret)
                append("refresh_token", refreshToken)
                append("grant_type", "refresh_token")
            },
        )

        if (response.status != HttpStatusCode.OK) {
            val errorBody = response.body<String>()
            throw DriveApiException("Token refresh failed (${response.status}): $errorBody")
        }

        return response.body<TokenResponse>()
    }
}

/**
 * Exception for Google Drive API errors.
 */
class DriveApiException(message: String, cause: Throwable? = null) : Exception(message, cause)

