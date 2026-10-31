package com.emusync.domain.sync

import kotlin.test.assertEquals
import org.junit.jupiter.api.Test

class TrackedSyncPlannerTest {
    private val time = "2026-10-01T12:00:00Z"
    private val original = "original-hash"
    private val localChange = "local-hash"
    private val cloudChange = "cloud-hash"

    private fun remote(hash: String? = original, version: String? = "1", date: String = time) =
        CloudSaveState("remote", date, sha256Checksum = hash, version = version)

    private fun previous() = SyncCheckpoint(original, "remote", time, original, "1")

    @Test
    fun `bootstrap requires evidence of equal content even with equal timestamps`() {
        assertEquals(SyncDecision.CONFLICT, planTrackedSync(localChange, remote(), null))
        assertEquals(SyncDecision.CONFLICT, planTrackedSync(original, remote(hash = null), null))
        assertEquals(SyncDecision.IN_SYNC, planTrackedSync(original, remote(), null))
    }

    @Test
    fun `clock ordering does not conceal local changes`() {
        assertEquals(SyncDecision.UPLOAD_LOCAL, planTrackedSync(localChange, remote(), previous()))
    }

    @Test
    fun `remote content change is detected even when timestamps and versions match`() {
        assertEquals(
            SyncDecision.DOWNLOAD_CLOUD,
            planTrackedSync(original, remote(cloudChange), previous()),
        )
        assertEquals(
            SyncDecision.CONFLICT,
            planTrackedSync(localChange, remote(cloudChange), previous()),
        )
    }

    @Test
    fun `independent changes to the same content converge without a conflict`() {
        assertEquals(
            SyncDecision.IN_SYNC,
            planTrackedSync(localChange, remote(localChange), previous()),
        )
    }

    @Test
    fun `metadata version is used when checksum is absent`() {
        assertEquals(
            SyncDecision.DOWNLOAD_CLOUD,
            planTrackedSync(original, remote(null, "2"), previous()),
        )
        assertEquals(
            SyncDecision.CONFLICT,
            planTrackedSync(localChange, remote(null, "2"), previous()),
        )
    }

    @Test
    fun `legacy remote metadata compares exact timestamps without tolerance`() {
        val checkpoint = previous().copy(remoteSha256 = null, remoteVersion = null)
        assertEquals(
            SyncDecision.IN_SYNC,
            planTrackedSync(original, remote(null, null), checkpoint),
        )
        assertEquals(
            SyncDecision.DOWNLOAD_CLOUD,
            planTrackedSync(
                original,
                remote(null, null, "2026-10-01T12:00:00.001Z"),
                checkpoint,
            ),
        )
    }

    @Test
    fun `remote replacement with a different id is a change`() {
        assertEquals(
            SyncDecision.CONFLICT,
            planTrackedSync(
                localChange,
                remote(null).copy(id = "replacement"),
                previous(),
            ),
        )
    }

    @Test
    fun `one missing copy is restored without propagating deletions`() {
        assertEquals(SyncDecision.UPLOAD_LOCAL, planTrackedSync(original, null, previous()))
        assertEquals(SyncDecision.DOWNLOAD_CLOUD, planTrackedSync(null, remote(), previous()))
        assertEquals(SyncDecision.IN_SYNC, planTrackedSync(null, null, previous()))
    }
}
