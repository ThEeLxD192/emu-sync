package com.emusync.domain.sync

fun canonicalCloudFiles(files: List<CloudFile>): List<CloudFile> =
    files
        .groupBy { it.name }
        .values
        .map { candidates ->
            if (
                candidates.size > 1 &&
                    (candidates.any { it.sha256Checksum == null } ||
                        candidates.map { it.sha256Checksum?.lowercase() }.distinct().size > 1)
            ) {
                throw SaveSyncException(
                    "Several cloud copies of '${candidates.first().name}' contain different or unverified progress. All copies were kept. Resolve the duplicate files in Drive before synchronizing."
                )
            }
            candidates.maxWith(
                compareBy<CloudFile> { file -> java.time.Instant.parse(file.modifiedTime) }
                    .thenBy { file -> file.id }
            )
        }
