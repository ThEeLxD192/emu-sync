package com.emusync.drive

import com.emusync.application.sync.CloudFolderMigrator
import com.emusync.config.ConfigManager
import com.emusync.domain.config.AppConfig
import com.emusync.domain.config.OAuthCredentials
import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import com.emusync.ui.AppStatus
import com.emusync.ui.createSaveSynchronizer
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class CloudFolderMigrationTest {
    @Test
    fun `files added by another device during merge remain in the source`() = runTest {
        MigrationDrive().use { drive ->
            drive.folder("source", "Old")
            drive.folder("destination", "New")
            drive.file("save", "save.dat", "source")
            drive.onMove = { drive.file("new-arrival", "offline-progress.dat", "source") }

            migrator(drive).migrate("token", listOf("Old"), listOf("New"))

            assertEquals("destination", drive.nodes.getValue("save").parent)
            assertEquals("source", drive.nodes.getValue("new-arrival").parent)
            assertTrue("source" in drive.nodes)
            assertTrue(drive.deletions.isEmpty())
        }
    }

    @Test
    fun `successful merge moves files and nested folders while keeping the source`(
        @TempDir directory: File
    ) = runTest {
        MigrationDrive().use { drive ->
            drive.folder("source", "Old")
            drive.folder("destination", "New")
            drive.file("save", "save.dat", "source")
            drive.folder("nested", "Nested", "source")
            drive.file("nested-save", "slot.dat", "nested")
            drive.file("existing", "other.dat", "destination")

            val old = NativePCGame("Old", "echo")
            assertTrue(
                orchestrator(drive, directory, old)
                    .renameEntryCloudData(old, old.copy(name = "New"))
            )

            assertEquals("destination", drive.nodes.getValue("save").parent)
            assertEquals("destination", drive.nodes.getValue("nested").parent)
            assertEquals("nested", drive.nodes.getValue("nested-save").parent)
            assertTrue(drive.children("source").isEmpty())
            assertTrue("source" in drive.nodes)
            assertTrue(drive.deletions.isEmpty())
        }
    }

    @Test
    fun `game rename refuses duplicate names before moving unique saves`(@TempDir directory: File) =
        runTest {
            MigrationDrive().use { drive ->
                drive.folder("console", "Console")
                drive.folder("source", "Old", "console")
                drive.folder("destination", "New", "console")
                drive.file("unique", "unique.dat", "source")
                drive.file("old-save", "save.dat", "source", "OLD PROGRESS")
                drive.file("new-save", "save.dat", "destination", "OTHER PROGRESS")
                val entry =
                    EmulatorSystem(
                        "Console",
                        "echo",
                        romsDirectory = directory.path,
                        extensions = listOf("rom"),
                    )
                val statuses = mutableListOf<AppStatus>()

                val success =
                    orchestrator(drive, directory, entry, statuses)
                        .renameGameCloudData(
                            entry,
                            "Game.rom",
                            "Old",
                            "New",
                            emptyList(),
                        )

                assertFalse(success)
                assertTrue(drive.moves.isEmpty())
                assertTrue(drive.deletions.isEmpty())
                assertEquals("OLD PROGRESS", drive.nodes.getValue("old-save").content)
                assertEquals("OTHER PROGRESS", drive.nodes.getValue("new-save").content)
                assertTrue((statuses.last() as AppStatus.Error).message.contains("save.dat"))
            }
        }

    @Test
    fun `matching subfolders are preserved instead of creating ambiguous duplicate folders`() =
        runTest {
            MigrationDrive().use { drive ->
                drive.folder("source", "Old")
                drive.folder("destination", "New")
                drive.folder("old-game", "Game", "source")
                drive.folder("new-game", "Game", "destination")

                assertFailsWith<com.emusync.domain.sync.SaveSyncException> {
                    migrator(drive).migrate("token", listOf("Old"), listOf("New"))
                }

                assertTrue(drive.moves.isEmpty())
                assertEquals("source", drive.nodes.getValue("old-game").parent)
                assertEquals("destination", drive.nodes.getValue("new-game").parent)
                assertTrue(drive.deletions.isEmpty())
            }
        }

    @Test
    fun `duplicate names already in source also block a merge`() = runTest {
        MigrationDrive().use { drive ->
            drive.folder("source", "Old")
            drive.folder("destination", "New")
            drive.file("first", "save.dat", "source")
            drive.file("second", "save.dat", "source")

            assertFailsWith<com.emusync.domain.sync.SaveSyncException> {
                migrator(drive).migrate("token", listOf("Old"), listOf("New"))
            }
            assertTrue(drive.moves.isEmpty())
        }
    }

    @Test
    fun `failed move restores previous moves and reports failure`(@TempDir directory: File) =
        runTest {
            MigrationDrive().use { drive ->
                drive.folder("source", "Old")
                drive.folder("destination", "New")
                drive.file("first", "first.dat", "source")
                drive.file("second", "second.dat", "source")
                drive.failMove = { it.id == "second" && it.to == "destination" }
                val entry = NativePCGame("Old", "echo")
                val statuses = mutableListOf<AppStatus>()

                assertFalse(
                    orchestrator(drive, directory, entry, statuses)
                        .renameEntryCloudData(entry, entry.copy(name = "New"))
                )

                assertEquals(
                    setOf("first", "second"),
                    drive.children("source").map { it.id }.toSet(),
                )
                assertTrue(drive.children("destination").isEmpty())
                assertEquals(
                    MigrationMove("first", "destination", "source"),
                    drive.moves.last(),
                )
                assertTrue(statuses.last() is AppStatus.Error)
                assertTrue(drive.deletions.isEmpty())

                // A retry after the transient failure must move the same IDs without duplication.
                drive.failMove = { false }
                assertTrue(
                    orchestrator(drive, directory, entry)
                        .renameEntryCloudData(entry, entry.copy(name = "New"))
                )
                assertEquals(
                    setOf("first", "second"),
                    drive.children("destination").map { it.id }.toSet(),
                )
            }
        }

    @Test
    fun `failed rollback retains files on both sides and identifies recovery location`() = runTest {
        MigrationDrive().use { drive ->
            drive.folder("source", "Old")
            drive.folder("destination", "New")
            drive.file("first", "first.dat", "source")
            drive.file("second", "second.dat", "source")
            drive.failMove = { it.id == "second" || it.to == "source" }

            val failure =
                assertFailsWith<com.emusync.domain.sync.SaveSyncException> {
                    migrator(drive).migrate("token", listOf("Old"), listOf("New"))
                }

            assertEquals("destination", drive.nodes.getValue("first").parent)
            assertEquals("source", drive.nodes.getValue("second").parent)
            assertTrue(failure.message!!.contains("New"))
            assertTrue(failure.message!!.contains("first.dat"))
            assertTrue(drive.deletions.isEmpty())
        }
    }

    @Test
    fun `cancellation restores acknowledged moves and propagates`(@TempDir directory: File) =
        runTest {
            MigrationDrive().use { drive ->
                drive.folder("source", "Old")
                drive.folder("destination", "New")
                drive.file("first", "first.dat", "source")
                drive.file("second", "second.dat", "source")
                drive.onMove = {
                    if (it.id == "second") throw CancellationException("Cancelled migration")
                }
                val entry = NativePCGame("Old", "echo")
                val statuses = mutableListOf<AppStatus>()

                assertFailsWith<CancellationException> {
                    orchestrator(drive, directory, entry, statuses)
                        .renameEntryCloudData(entry, entry.copy(name = "New"))
                }

                assertEquals(
                    setOf("first", "second"),
                    drive.children("source").map { it.id }.toSet(),
                )
                assertFalse(statuses.any { it is AppStatus.Error })
                assertTrue(drive.deletions.isEmpty())
            }
        }

    @Test
    fun `native group change moves only its game folder`(@TempDir directory: File) = runTest {
        MigrationDrive().use { drive ->
            drive.folder("old-group", "Old Group")
            drive.folder("new-group", "New Group")
            drive.folder("game", "Game", "old-group")
            drive.folder("other", "Other", "old-group")
            drive.file("save", "save.dat", "game")
            val entry = NativePCGame("Game", "echo", group = "Old Group")
            val other = NativePCGame("Other", "echo", group = "Old Group")

            assertTrue(
                orchestrator(drive, directory, entry, otherEntries = listOf(other))
                    .renameEntryCloudData(entry, entry.copy(group = "New Group"))
            )

            assertEquals("new-group", drive.nodes.getValue("game").parent)
            assertEquals("old-group", drive.nodes.getValue("other").parent)
            assertEquals("game", drive.nodes.getValue("save").parent)
            assertTrue(drive.deletions.isEmpty())
        }
    }

    @Test
    fun `paths resolving to the same folder do not move or delete its contents`() = runTest {
        MigrationDrive().use { drive ->
            drive.folder("source", "Old")
            drive.file("save", "save.dat", "source")

            migrator(drive).migrate("token", listOf("Old "), listOf("Old"))

            assertTrue(drive.mutations.isEmpty())
            assertEquals("source", drive.nodes.getValue("save").parent)
        }
    }

    private fun migrator(drive: MigrationDrive) =
        CloudFolderMigrator(DriveFolders(drive.client), DriveFileListing(DriveSearch(drive.client)))

    private fun orchestrator(
        drive: MigrationDrive,
        directory: File,
        entry: GameEntry,
        statuses: MutableList<AppStatus> = mutableListOf(),
        otherEntries: List<GameEntry> = emptyList(),
    ) =
        createSaveSynchronizer(
            drive.client,
            AppConfig(
                OAuthCredentials("client", "secret", "refresh"),
                listOf(entry) + otherEntries,
            ),
            ConfigManager(File(directory, "config.json")),
            onStatus = { statuses += it },
        )
}
