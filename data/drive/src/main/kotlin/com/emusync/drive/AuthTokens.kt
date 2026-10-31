package com.emusync.drive

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * Full token response from the initial authorization code exchange. Includes the [refreshToken]
 * which must be persisted to config.json.
 */
@Serializable
internal data class AuthTokens(
    @SerialName("access_token") val accessToken: String,
    @SerialName("refresh_token") val refreshToken: String,
    @SerialName("expires_in") val expiresIn: Int,
    @SerialName("token_type") val tokenType: String,
)
