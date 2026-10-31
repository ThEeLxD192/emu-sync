package com.emusync.application.connection

import com.emusync.domain.config.AppConfig

data class ConnectionResult(
    val config: AppConfig?,
    val message: String,
    val authorized: Boolean = false,
)
