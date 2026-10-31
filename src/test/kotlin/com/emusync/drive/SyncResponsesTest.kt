package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetType
import com.emusync.domain.sync.SaveConflict
import com.emusync.domain.sync.SaveSynchronizer
import com.emusync.domain.sync.SyncDecision
import com.emusync.domain.sync.SyncEvent
import com.emusync.model.AppConfig
import com.emusync.model.GoogleDriveConfig
import java.io.File
import java.time.Instant
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.yield
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class SyncResponsesTest {
    @Test
    fun `cancelled conflict reply cannot resolve an identical later request or write saves`(
        @TempDir directory: File
    ) = runTest {
        SaveMappingDrive().use { drive ->
            drive.add("remote", "save.dat", "CLOUD")
            val local =
                File(directory, "save.dat").apply {
                    writeText("LOCAL")
                    setLastModified(
                        Instant.parse(SaveMappingDrive.CLOUD_TIME).plusSeconds(60).toEpochMilli()
                    )
                }
            val entry =
                NativePCGame(
                    "Game",
                    "echo",
                    saveTargets = listOf(SaveTarget(local.path, SaveTargetType.FILE)),
                )
            val conflicts = Channel<SaveConflict>(Channel.UNLIMITED)
            val sync: SaveSynchronizer =
                SyncOrchestrator(
                    drive.client,
                    AppConfig(GoogleDriveConfig("client", "secret", "refresh"), listOf(entry)),
                    ConfigManager(File(directory, "config.json")),
                ) {
                    if (it is SyncEvent.Conflict) check(conflicts.trySend(it.request).isSuccess)
                }

            val first = async { sync.syncEntry(entry) }
            val cancelledRequest = conflicts.receive()
            first.cancelAndJoin()
            assertTrue(first.isCancelled)
            val second = async { sync.syncEntry(entry) }
            val currentRequest = conflicts.receive()
            assertEquals(cancelledRequest, currentRequest)
            assertTrue(cancelledRequest !== currentRequest)
            assertEquals(local.lastModified(), currentRequest.localModifiedTimeMs)
            assertEquals(
                Instant.parse(SaveMappingDrive.CLOUD_TIME).toEpochMilli(),
                currentRequest.cloudModifiedTimeMs,
            )

            sync.resolveConflict(cancelledRequest, SyncDecision.UPLOAD_LOCAL)
            yield()
            assertFalse(second.isCompleted)
            assertTrue(drive.downloads.isEmpty())
            assertTrue(drive.uploadedNames.isEmpty())
            assertEquals("LOCAL", local.readText())
            assertEquals("CLOUD", drive.contents.getValue("remote"))

            sync.resolveConflict(currentRequest, SyncDecision.DOWNLOAD_CLOUD)
            assertTrue(second.await())
            assertEquals("CLOUD", local.readText())
            assertEquals(listOf("remote"), drive.downloads)
            assertTrue(drive.uploadedNames.isEmpty())
            conflicts.close()
        }
    }
}
