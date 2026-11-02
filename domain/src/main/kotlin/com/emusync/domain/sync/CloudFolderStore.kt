package com.emusync.domain.sync

/** Hierarchy operations needed to locate saves and migrate their folders without data loss. */
interface CloudFolderStore {
    suspend fun ensurePath(accessToken: String, pathSegments: List<String>): String

    suspend fun findPath(accessToken: String, pathSegments: List<String>): String?

    suspend fun renameFolder(
        accessToken: String,
        folderId: String,
        newName: String,
        addParentId: String? = null,
        removeParentId: String? = null,
    )

    suspend fun moveFile(
        accessToken: String,
        fileId: String,
        addParentId: String,
        removeParentId: String,
    )
}
