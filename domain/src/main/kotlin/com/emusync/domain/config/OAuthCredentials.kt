package com.emusync.domain.config

/** OAuth client credentials and refresh token, independent of provider JSON. */
data class OAuthCredentials(
    val clientId: String,
    val clientSecret: String,
    val refreshToken: String? = null,
)
