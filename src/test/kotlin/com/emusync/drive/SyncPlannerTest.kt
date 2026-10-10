package com.emusync.drive

import org.junit.jupiter.api.Test
import java.time.Instant
import kotlin.test.assertEquals

class SyncPlannerTest {
    private val base = Instant.parse("2026-10-09T12:00:00Z")

    @Test
    fun `planner creates deterministic actions for local cloud and shared files`() {
        val plan = planDirectorySync(
            localFiles = listOf(
                LocalFileState("local-only.sav", base.toEpochMilli()),
                LocalFileState("shared.sav", base.plusSeconds(10).toEpochMilli()),
            ),
            cloudFiles = listOf(
                CloudFileState("cloud", "cloud-only.sav", base.toString()),
                CloudFileState("shared", "shared.sav", base.toString()),
            ),
        )

        assertEquals(
            listOf(
                "cloud-only.sav" to SyncDecision.DOWNLOAD_CLOUD,
                "local-only.sav" to SyncDecision.UPLOAD_LOCAL,
                "shared.sav" to SyncDecision.CONFLICT,
            ),
            plan.map { it.relativePath to it.decision },
        )
    }

    @Test
    fun `post-play planning prefers newer local progress`() {
        val plan = planDirectorySync(
            localFiles = listOf(LocalFileState("save.dat", base.plusMillis(1).toEpochMilli())),
            cloudFiles = listOf(CloudFileState("id", "save.dat", base.toString())),
            localNewerAction = LocalNewerAction.UPLOAD,
        )

        assertEquals(SyncDecision.UPLOAD_LOCAL, plan.single().decision)
    }

    @Test
    fun `duplicate cloud paths choose the newest file independent of input order`() {
        val older = CloudFileState("file-a", "save.dat", base.toString())
        val newer = CloudFileState("file-b", "save.dat", base.plusSeconds(10).toString())

        val forward = planDirectorySync(emptyList(), listOf(older, newer)).single()
        val reversed = planDirectorySync(emptyList(), listOf(newer, older)).single()

        assertEquals("file-b", forward.cloud?.id)
        assertEquals(listOf("file-a"), forward.duplicateCloudFiles.map { it.id })
        assertEquals(forward, reversed)
    }
}
