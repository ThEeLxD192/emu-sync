package com.emusync.domain.config

/** Authorizations are valid only while both credentials and their connection revision match. */
interface CredentialStore {
    fun snapshotCredentials(fallback: AppConfig): CredentialSnapshot

    fun compareAndSetCredentials(
        expected: CredentialSnapshot,
        credentials: OAuthCredentials?,
        fallback: AppConfig,
    ): Boolean
}
