package com.emusync.domain.sync

data class PlannedFileAction(
    val relativePath: String,
    val local: LocalFileState?,
    val cloud: CloudFileState?,
    val duplicateCloudFiles: List<CloudFileState>,
    val decision: SyncDecision,
)
