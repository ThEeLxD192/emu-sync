package com.emusync.domain.sync

interface CloudFileListing {
    suspend fun listFilesInFolder(accessToken: String, folderId: String): List<CloudFile>

    suspend fun listChildrenInFolder(accessToken: String, folderId: String): List<CloudFile>
}
