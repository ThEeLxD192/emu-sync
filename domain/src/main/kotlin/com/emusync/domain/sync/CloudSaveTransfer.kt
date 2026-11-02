package com.emusync.domain.sync

/** Transfers bytes through a cloud provider without exposing its wire DTOs or HTTP client. */
interface CloudSaveTransfer {
    suspend fun downloadSave(
        accessToken: String,
        fileId: String,
        destinationPath: String,
        modifiedTime: String?,
        progressFileName: String,
    )

    suspend fun uploadSave(
        accessToken: String,
        sourcePath: String,
        remoteName: String,
        parentFolderId: String,
    ): CloudSaveState

    suspend fun replaceSave(
        accessToken: String,
        observed: CloudSaveState,
        sourcePath: String,
        remoteName: String,
    ): CloudSaveState
}
