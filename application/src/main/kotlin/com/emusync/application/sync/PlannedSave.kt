package com.emusync.application.sync

import com.emusync.domain.sync.CloudFile
import com.emusync.domain.sync.LocalSavePath

/** Identifies both copies of one save and its exact remote destination. */
internal data class PlannedSave(
    val local: LocalSavePath,
    val remote: CloudFile?,
    val folderId: String,
    val driveName: String,
)
