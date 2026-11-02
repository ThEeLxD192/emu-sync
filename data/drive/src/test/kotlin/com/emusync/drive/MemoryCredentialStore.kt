package com.emusync.drive

import com.emusync.domain.config.AppConfig
import com.emusync.domain.config.CredentialSnapshot
import com.emusync.domain.config.CredentialStore
import com.emusync.domain.config.OAuthCredentials

internal class MemoryCredentialStore(var config: AppConfig = AppConfig()) : CredentialStore {
    private var revision = 0L

    fun replace(config: AppConfig) {
        this.config = config
        revision++
    }

    override fun snapshotCredentials(fallback: AppConfig) =
        CredentialSnapshot(config.cloudCredentials, revision)

    override fun compareAndSetCredentials(
        expected: CredentialSnapshot,
        credentials: OAuthCredentials?,
        fallback: AppConfig,
    ): Boolean {
        if (snapshotCredentials(fallback) != expected) return false
        if (config.cloudCredentials != credentials)
            replace(config.copy(cloudCredentials = credentials))
        return true
    }
}
