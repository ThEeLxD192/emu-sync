package com.emusync.drive

import com.emusync.domain.sync.SaveSyncException
import com.emusync.domain.sync.SyncDecision
import com.emusync.domain.sync.canonicalCloudFiles
import com.emusync.local.saves.SyncHistoryStore
import com.emusync.local.saves.TrackedFileSync
import io.ktor.client.engine.mock.toByteArray
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class ConditionalCloudWriteTest {
    private val time = "2026-10-01T12:00:00Z"
    private val original = sha256("ORIGINAL".toByteArray())

    private fun observed() =
        DriveFile("remote", "save.dat", time, version = "1", sha256Checksum = original)

    @Test
    fun `concurrent remote change keeps local cloud and previous synchronization history`(
        @TempDir directory: File
    ) = runTest {
        SaveMappingDrive().use { drive ->
            drive.add("remote", "save.dat", "ORIGINAL")
            val local = File(directory, "save.dat").apply { writeText("ORIGINAL") }
            val history = SyncHistoryStore(File(directory, "config.json"))
            val sync = TrackedFileSync(DriveTransfer(drive.client), history)
            sync.remember(
                (local).path,
                (drive.files.getValue("remote")).toSyncState(),
                "game",
                "save.dat",
            )
            val key = history.key(local, "game", "save.dat")
            val previous = history.get(key)
            local.writeText("LOCAL")
            drive.beforeConditionalCommit = { drive.add("remote", "save.dat", "CLOUD") }
            assertFailsWith<SaveSyncException> {
                sync.upload(
                    "token",
                    (local).path,
                    drive.files.getValue("remote").toSyncState(),
                    "game",
                    "save.dat",
                )
            }
            assertEquals("LOCAL", local.readText())
            assertEquals("CLOUD", drive.contents.getValue("remote"))
            assertEquals(previous, history.get(key))
            assertEquals(
                SyncDecision.CONFLICT,
                sync.plan(
                    (local).path,
                    drive.files.getValue("remote").toSyncState(),
                    "game",
                    "save.dat",
                ),
            )
            assertTrue(drive.uploadedNames.isEmpty())
        }
    }

    @Test
    fun `simultaneous initial creates retain both copies and divergent duplicates block selection`(
        @TempDir directory: File
    ) = runTest {
        SaveMappingDrive().use { drive ->
            val first =
                File(directory, "first/save.dat").apply {
                    parentFile.mkdirs()
                    writeText("FIRST")
                }
            val second =
                File(directory, "second/save.dat").apply {
                    parentFile.mkdirs()
                    writeText("SECOND")
                }
            val history = SyncHistoryStore(File(directory, "config.json"))
            val sync = TrackedFileSync(DriveTransfer(drive.client), history)
            // Both devices observed an absent remote file before either creation.
            sync.upload("token", (first).path, null, "game", "save.dat")
            sync.upload("token", (second).path, null, "game", "save.dat")
            assertEquals(setOf("FIRST", "SECOND"), drive.contents.values.toSet())
            assertFailsWith<SaveSyncException> {
                canonicalCloudFiles(drive.files.values.map { it.toCloudFile() })
            }
            assertEquals(2, drive.files.size)
            assertEquals("FIRST", first.readText())
            assertEquals("SECOND", second.readText())
        }
    }

    @Test
    fun `identical duplicate content remains safe to compare but unknown content does not`() {
        val file = observed().toCloudFile()
        assertEquals(1, canonicalCloudFiles(listOf(file, file.copy(id = "other"))).size)
        assertFailsWith<SaveSyncException> {
            canonicalCloudFiles(listOf(file, file.copy(id = "other", sha256Checksum = null)))
        }
    }
}
