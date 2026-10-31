package com.emusync.drive

import com.emusync.domain.sync.CloudFile
import com.emusync.domain.sync.CloudFileListing

class DriveFileListing(private val search: DriveSearch) : CloudFileListing {
    override suspend fun listFilesInFolder(accessToken: String, folderId: String) =
        search.listFilesInFolder(accessToken, folderId).map(DriveFile::toCloudFile)

    override suspend fun listChildrenInFolder(accessToken: String, folderId: String) =
        search.listChildrenInFolder(accessToken, folderId).map(DriveFile::toCloudFile)
}

fun DriveFile.toCloudFile() = CloudFile(id, name, modifiedTime, isFolder, sha256Checksum, version)
