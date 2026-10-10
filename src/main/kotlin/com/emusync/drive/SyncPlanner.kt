package com.emusync.drive

/** Immutable metadata used to plan a sync without performing file or network I/O. */
data class LocalFileState(val relativePath: String, val modifiedTimeMs: Long)

data class CloudFileState(val id: String, val relativePath: String, val modifiedTimeIso: String)

data class PlannedFileAction(
    val relativePath: String,
    val local: LocalFileState?,
    val cloud: CloudFileState?,
    val decision: SyncDecision,
)

/** Produces one deterministic action for every local or cloud file in a save target. */
fun planDirectorySync(
    localFiles: List<LocalFileState>,
    cloudFiles: List<CloudFileState>,
    localNewerAction: LocalNewerAction = LocalNewerAction.RESOLVE_CONFLICT,
): List<PlannedFileAction> {
    val localByPath = localFiles.associateBy { it.relativePath }
    val cloudByPath = cloudFiles.associateBy { it.relativePath }
    return (localByPath.keys + cloudByPath.keys).sorted().map { relativePath ->
        val local = localByPath[relativePath]
        val cloud = cloudByPath[relativePath]
        PlannedFileAction(
            relativePath = relativePath,
            local = local,
            cloud = cloud,
            decision = planSyncDecision(
                localModifiedTimeMs = local?.modifiedTimeMs,
                cloudModifiedTimeIso = cloud?.modifiedTimeIso,
                localNewerAction = localNewerAction,
            ),
        )
    }
}
