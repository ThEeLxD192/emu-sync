package com.emusync.domain.config

/** Atomic configuration updates retain credentials and unrelated concurrent library edits. */
interface ConfigurationStore {
    fun loadConfig(): AppConfig

    fun updateConfig(transform: (AppConfig) -> AppConfig): AppConfig

    fun updateCredentials(transform: (OAuthCredentials?) -> OAuthCredentials?): AppConfig
}
