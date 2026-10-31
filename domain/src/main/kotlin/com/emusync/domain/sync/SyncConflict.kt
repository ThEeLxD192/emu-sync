package com.emusync.domain.sync

import java.time.Duration
import java.time.Instant

/**
 * Selects the synchronization action for a local/cloud file pair.
 *
 * Missing timestamps represent a missing side. When both sides exist, the established timestamp
 * conflict policy is used by [resolveConflict].
 */
fun planSyncDecision(
    localModifiedTimeMs: Long?,
    cloudModifiedTimeIso: String?,
    conflictThresholdSeconds: Long = 5L,
    localNewerAction: LocalNewerAction = LocalNewerAction.RESOLVE_CONFLICT,
): SyncDecision {
    return when {
        localModifiedTimeMs == null && cloudModifiedTimeIso == null -> SyncDecision.IN_SYNC
        localModifiedTimeMs == null -> SyncDecision.DOWNLOAD_CLOUD
        cloudModifiedTimeIso == null -> SyncDecision.UPLOAD_LOCAL
        localNewerAction == LocalNewerAction.UPLOAD &&
            localModifiedTimeMs > Instant.parse(cloudModifiedTimeIso).toEpochMilli() ->
            SyncDecision.UPLOAD_LOCAL
        else -> resolveConflict(localModifiedTimeMs, cloudModifiedTimeIso, conflictThresholdSeconds)
    }
}

/**
 * Pure function that determines the sync action based on file timestamps.
 *
 * This contains NO I/O — it's a deterministic decision function that can be easily unit-tested.
 *
 * @param localModifiedTime Last modified time of the local save file.
 * @param cloudModifiedTime `modifiedTime` from the Google Drive API (ISO 8601).
 * @param conflictThresholdSeconds If both files were modified within this window of each other,
 *   treat it as a conflict. Default: 5 seconds.
 * @return [SyncDecision] indicating what action to take.
 */
fun resolveConflict(
    localModifiedTime: Instant,
    cloudModifiedTime: Instant,
    conflictThresholdSeconds: Long = 5L,
): SyncDecision {
    val difference = Duration.between(cloudModifiedTime, localModifiedTime)
    val threshold = Duration.ofSeconds(conflictThresholdSeconds)

    return when {
        // Within threshold → basically in sync
        difference.abs() <= threshold -> SyncDecision.IN_SYNC

        // Cloud is clearly newer → download
        difference.isNegative -> SyncDecision.DOWNLOAD_CLOUD

        // Local is newer → this means local was modified offline (or without syncing).
        // Since we don't have a sync state database, we ask the user to resolve the conflict
        // to avoid overwriting cloud progress from another device.
        else -> SyncDecision.CONFLICT
    }
}

/**
 * Convenience overload that parses the Drive API's ISO 8601 string.
 *
 * @param localModifiedTimeMs Local file's `lastModified()` in epoch milliseconds.
 * @param cloudModifiedTimeIso ISO 8601 string from Drive API (e.g. "2024-01-15T10:30:00.000Z").
 * @param conflictThresholdSeconds Maximum timestamp difference considered in sync.
 */
fun resolveConflict(
    localModifiedTimeMs: Long,
    cloudModifiedTimeIso: String,
    conflictThresholdSeconds: Long = 5L,
): SyncDecision {
    val localInstant = Instant.ofEpochMilli(localModifiedTimeMs)
    val cloudInstant = Instant.parse(cloudModifiedTimeIso)
    return resolveConflict(localInstant, cloudInstant, conflictThresholdSeconds)
}
