package com.emusync.drive

import java.time.Duration
import java.time.Instant
import kotlin.math.abs

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

/** Determines how a newer local copy is handled by [planSyncDecision]. */
enum class LocalNewerAction {
    /** Ask for conflict resolution when the local copy is clearly newer. */
    RESOLVE_CONFLICT,

    /** Prefer local progress, as in the post-play upload step. */
    UPLOAD,
}

/**
 * Selects the synchronization action for a local/cloud file pair.
 *
 * Missing timestamps represent a missing side. When both sides exist, the
 * established timestamp conflict policy is used by [resolveConflict].
 */
fun planSyncDecision(
    localModifiedTimeMs: Long?,
    cloudModifiedTimeIso: String?,
    conflictThresholdSeconds: Long = 5L,
    localNewerAction: LocalNewerAction = LocalNewerAction.RESOLVE_CONFLICT,
): SyncDecision {
    if (localModifiedTimeMs == null && cloudModifiedTimeIso == null) return SyncDecision.IN_SYNC
    if (localModifiedTimeMs == null) return SyncDecision.DOWNLOAD_CLOUD
    if (cloudModifiedTimeIso == null) return SyncDecision.UPLOAD_LOCAL

    val cloudModifiedTimeMs = Instant.parse(cloudModifiedTimeIso).toEpochMilli()
    if (localNewerAction == LocalNewerAction.UPLOAD && localModifiedTimeMs > cloudModifiedTimeMs) {
        return SyncDecision.UPLOAD_LOCAL
    }

    return resolveConflict(localModifiedTimeMs, cloudModifiedTimeIso, conflictThresholdSeconds)
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
    val diffSeconds = Duration.between(cloudModifiedTime, localModifiedTime).seconds

    return when {
        // Within threshold → basically in sync
        abs(diffSeconds) <= conflictThresholdSeconds -> SyncDecision.IN_SYNC

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
