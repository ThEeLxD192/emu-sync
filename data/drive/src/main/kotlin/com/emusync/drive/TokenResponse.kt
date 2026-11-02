package com.emusync.drive

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Token response from the refresh flow (no refresh_token included). */
@Serializable
internal data class TokenResponse(
    @SerialName("access_token") val accessToken: String,
    @SerialName("expires_in") val expiresIn: Int,
    @SerialName("token_type") val tokenType: String,
)
