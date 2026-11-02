package com.emusync.domain.sync

import java.time.Instant

/** Produces one deterministic action for every local or cloud file in a save target. */
fun planDirectorySync(
    localFiles: List<LocalFileState>,
    cloudFiles: List<CloudFileState>,
    localNewerAction: LocalNewerAction = LocalNewerAction.RESOLVE_CONFLICT,
): List<PlannedFileAction> {
    val localByPath = localFiles.associateBy { it.relativePath }
    val cloudByPath =
        cloudFiles
            .groupBy { it.relativePath }
            .mapValues { (_, files) ->
                files.sortedWith(
                    compareBy<CloudFileState> { Instant.parse(it.modifiedTimeIso) }.thenBy { it.id }
                )
            }
    return (localByPath.keys + cloudByPath.keys).sorted().map { relativePath ->
        val local = localByPath[relativePath]
        val cloudCandidates = cloudByPath[relativePath].orEmpty()
        val cloud = cloudCandidates.lastOrNull()
        PlannedFileAction(
            relativePath = relativePath,
            local = local,
            cloud = cloud,
            duplicateCloudFiles = cloudCandidates.dropLast(1),
            decision =
                planSyncDecision(
                    localModifiedTimeMs = local?.modifiedTimeMs,
                    cloudModifiedTimeIso = cloud?.modifiedTimeIso,
                    localNewerAction = localNewerAction,
                ),
        )
    }
}
