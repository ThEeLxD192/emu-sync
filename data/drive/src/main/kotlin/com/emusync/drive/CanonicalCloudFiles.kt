package com.emusync.drive

import com.emusync.domain.sync.SaveSyncException
import java.time.Instant

fun canonicalCloudFiles(files: List<DriveFile>): List<DriveFile> =
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
                compareBy<DriveFile> { file -> java.time.Instant.parse(file.modifiedTime) }
                    .thenBy { file -> file.id }
            )
        }
