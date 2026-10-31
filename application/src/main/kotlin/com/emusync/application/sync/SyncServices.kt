package com.emusync.application.sync

import com.emusync.domain.config.CloudAuthorization
import com.emusync.domain.execution.GameLauncher
import com.emusync.domain.sync.CloudFileListing
import com.emusync.domain.sync.CloudFolderStore
import com.emusync.domain.sync.LocalSaveAccess
import com.emusync.domain.sync.LocalSaveFiles

/** Borrowed capabilities. The desktop composition owns their resources. */
data class SyncServices(
    val files: LocalSaveAccess,
    val search: CloudFileListing,
    val folders: CloudFolderStore,
    val trackedSync: LocalSaveFiles,
    val authorization: CloudAuthorization,
    val runner: GameLauncher,
)
