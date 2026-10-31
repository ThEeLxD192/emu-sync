package com.emusync.domain.config

fun interface CloudAuthorization {
    suspend fun authorize(config: AppConfig, allowInteractive: Boolean): String
}
