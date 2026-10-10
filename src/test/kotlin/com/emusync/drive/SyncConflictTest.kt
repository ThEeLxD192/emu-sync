package com.emusync.drive

import org.junit.jupiter.api.Test
import java.time.Instant
import kotlin.test.assertEquals

class SyncConflictTest {

    private val baseTime = Instant.parse("2026-03-18T12:00:00Z")

    @Test
    fun `local file significantly newer should resolve to CONFLICT`() {
        val decision = resolveConflict(
            localModifiedTime = baseTime.plusSeconds(3600),
            cloudModifiedTime = baseTime,
        )
        assertEquals(SyncDecision.CONFLICT, decision)
    }

    @Test
    fun `cloud file significantly newer should resolve to DOWNLOAD_CLOUD`() {
        val decision = resolveConflict(
            localModifiedTime = baseTime,
            cloudModifiedTime = baseTime.plusSeconds(3600),
        )
        assertEquals(SyncDecision.DOWNLOAD_CLOUD, decision)
    }

    @Test
    fun `identical timestamps should resolve to IN_SYNC`() {
        val decision = resolveConflict(
            localModifiedTime = baseTime,
            cloudModifiedTime = baseTime,
        )
        assertEquals(SyncDecision.IN_SYNC, decision)
    }

    @Test
    fun `timestamps within tolerance threshold (3s) should resolve to IN_SYNC`() {
        val decision = resolveConflict(
            localModifiedTime = baseTime.plusSeconds(3),
            cloudModifiedTime = baseTime,
            conflictThresholdSeconds = 5L,
        )
        assertEquals(SyncDecision.IN_SYNC, decision)
    }

    @Test
    fun `timestamps exactly at boundary should resolve to IN_SYNC`() {
        val decision = resolveConflict(
            localModifiedTime = baseTime.plusSeconds(5),
            cloudModifiedTime = baseTime,
            conflictThresholdSeconds = 5L,
        )
        assertEquals(SyncDecision.IN_SYNC, decision)
    }

    @Test
    fun `local timestamps beyond threshold (6s) should resolve to CONFLICT`() {
        val decision = resolveConflict(
            localModifiedTime = baseTime.plusSeconds(6),
            cloudModifiedTime = baseTime,
            conflictThresholdSeconds = 5L,
        )
        assertEquals(SyncDecision.CONFLICT, decision)
    }

    @Test
    fun `fractional timestamps beyond threshold are not truncated`() {
        assertEquals(
            SyncDecision.CONFLICT,
            resolveConflict(baseTime.plusMillis(5_001), baseTime, conflictThresholdSeconds = 5L),
        )
        assertEquals(
            SyncDecision.DOWNLOAD_CLOUD,
            resolveConflict(baseTime, baseTime.plusMillis(5_900), conflictThresholdSeconds = 5L),
        )
    }

    @Test
    fun `convenience overload with epoch millis and ISO string works correctly`() {
        val decision = resolveConflict(
            localModifiedTimeMs = baseTime.toEpochMilli(),
            cloudModifiedTimeIso = "2026-03-18T13:00:00.000Z", // cloud is 1h newer
        )
        assertEquals(SyncDecision.DOWNLOAD_CLOUD, decision)
    }

    @Test
    fun `plan chooses no action when neither copy exists`() {
        assertEquals(SyncDecision.IN_SYNC, planSyncDecision(null, null))
    }

    @Test
    fun `plan downloads when only cloud copy exists`() {
        assertEquals(
            SyncDecision.DOWNLOAD_CLOUD,
            planSyncDecision(null, "2026-03-18T12:00:00Z"),
        )
    }

    @Test
    fun `plan uploads when only local copy exists`() {
        assertEquals(
            SyncDecision.UPLOAD_LOCAL,
            planSyncDecision(baseTime.toEpochMilli(), null),
        )
    }

    @Test
    fun `plan delegates to timestamp conflict policy when both copies exist`() {
        assertEquals(
            SyncDecision.CONFLICT,
            planSyncDecision(
                localModifiedTimeMs = baseTime.plusSeconds(6).toEpochMilli(),
                cloudModifiedTimeIso = baseTime.toString(),
            ),
        )
    }

    @Test
    fun `post-play policy uploads even when local timestamp is inside conflict window`() {
        assertEquals(
            SyncDecision.UPLOAD_LOCAL,
            planSyncDecision(
                localModifiedTimeMs = baseTime.plusSeconds(1).toEpochMilli(),
                cloudModifiedTimeIso = baseTime.toString(),
                localNewerAction = LocalNewerAction.UPLOAD,
            ),
        )
    }
}
