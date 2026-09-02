package com.emusync.drive

import java.time.Instant

/**
 * The result of comparing a local save file with its cloud counterpart.
 */
enum class SyncDecision {
    /** Local file is newer → should upload to Drive. */
    UPLOAD_LOCAL,

    /** Cloud file is newer → should download from Drive. */
    DOWNLOAD_CLOUD,

    /** Both have the same modification time → no action needed. */
    IN_SYNC,

    /** Timestamps are too close to call or both modified since last sync → ask the user. */
    CONFLICT,
}

/**
 * Pure function that determines the sync action based on file timestamps.
 *
 * This contains NO I/O — it's a deterministic decision function that can be
 * easily unit-tested.
 *
 * @param localModifiedTime  Last modified time of the local save file.
 * @param cloudModifiedTime  `modifiedTime` from the Google Drive API (ISO 8601).
 * @param conflictThresholdSeconds  If both files were modified within this window
 *                                   of each other, treat it as a conflict. Default: 5 seconds.
 * @return [SyncDecision] indicating what action to take.
 */
fun resolveConflict(
    localModifiedTime: Instant,
    cloudModifiedTime: Instant,
    conflictThresholdSeconds: Long = 5L,
): SyncDecision {
    val diffSeconds = java.time.Duration.between(cloudModifiedTime, localModifiedTime).seconds

    return when {
        // Within threshold → basically in sync
        kotlin.math.abs(diffSeconds) <= conflictThresholdSeconds -> SyncDecision.IN_SYNC

        // Cloud is clearly newer → download
        diffSeconds < 0 -> SyncDecision.DOWNLOAD_CLOUD

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
