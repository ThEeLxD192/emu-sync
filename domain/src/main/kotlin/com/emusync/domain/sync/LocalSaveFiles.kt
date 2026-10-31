package com.emusync.domain.sync

/** Safe local snapshots, history and recovery copies around cloud transfers. */
interface LocalSaveFiles {
    suspend fun plan(
        localPath: String,
        remote: CloudSaveState?,
        folderId: String,
        remoteName: String,
    ): SyncDecision

    suspend fun remember(
        localPath: String,
        remote: CloudSaveState,
        folderId: String,
        remoteName: String,
    )

    suspend fun download(
        token: String,
        remote: CloudSaveState,
        localPath: String,
        folderId: String,
        remoteName: String,
    )

    suspend fun upload(
        token: String,
        localPath: String,
        remote: CloudSaveState?,
        folderId: String,
        remoteName: String,
    )
}
