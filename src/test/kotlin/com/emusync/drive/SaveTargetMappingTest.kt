package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.model.AppConfig
import com.emusync.model.EmulatorSystem
import com.emusync.model.GoogleDriveConfig
import com.emusync.model.NativePCGame
import com.emusync.model.SaveTarget
import com.emusync.model.SaveTargetType
import com.emusync.model.scopeNewSaveTargets
import com.emusync.ui.AppStatus
import com.emusync.ui.CloudSyncStatus
import com.emusync.ui.GameItem
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import java.time.Instant
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class SaveTargetMappingTest {
    @Test
    fun `new directory scopes upload and restore without depending on local folder names`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            val first = File(directory, "original-a/SAVE").apply { mkdirs() }
            val second = File(directory, "original-b/SAVE").apply { mkdirs() }
            seed(File(first, "save.dat"), "FIRST")
            seed(File(second, "save.dat"), "SECOND")
            val targets = scopeNewSaveTargets(listOf(
                SaveTarget(first.path, SaveTargetType.DIRECTORY), SaveTarget(second.path, SaveTargetType.DIRECTORY),
            ))
            val original = entry(targets)

            assertTrue(orchestrator(drive, directory, original).syncEntry(original))
            assertEquals(setOf("save-target-1/save.dat", "save-target-2/save.dat"), drive.uploadedNames.toSet())

            val restored = original.copy(saveTargets = targets.reversed().map {
                it.copy(path = File(directory, "restored/${it.cloudPath}").path)
            })
            assertTrue(orchestrator(drive, directory, restored).syncEntry(restored))

            assertEquals("FIRST", File(directory, "restored/save-target-1/save.dat").readText())
            assertEquals("SECOND", File(directory, "restored/save-target-2/save.dat").readText())
            assertEquals(2, drive.uploadedNames.size)
            assertEquals(2, drive.files.size)
        }
    }

    @Test
    fun `directory restore rejects a remote file that is also a parent path`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            drive.add("file", "progress/slot", "FILE")
            drive.add("nested", "progress/slot/save.dat", "NESTED FILE")
            val target = SaveTarget(File(directory, "restore").path, SaveTargetType.DIRECTORY, "progress")
            val entry = entry(listOf(target))

            assertFalse(orchestrator(drive, directory, entry).syncEntry(entry))
            assertFalse(File(target.path).exists())
            assertTrue(drive.downloads.isEmpty())
        }
    }

    @Test
    fun `legacy target migration preserves cloud paths globally and per ROM`(@TempDir directory: File) {
        val localDirectory = File(directory, "saves").apply { mkdirs() }
        val localFile = File(directory, "save.dat").apply { writeText("SAVE") }
        val system = EmulatorSystem(
            "Console", "echo", romsDirectory = directory.path, extensions = listOf("rom"),
            saveTargets = listOf(SaveTarget(localDirectory.path, SaveTargetType.UNKNOWN, "global")),
            saveTargetsByRom = mapOf("Game.rom" to listOf(SaveTarget(localFile.path, SaveTargetType.UNKNOWN, "profiles/main/save.dat"))),
        )
        val manager = ConfigManager(File(directory, "config.json"))
        manager.save(AppConfig(entries = listOf(system)))

        val loaded = manager.load().entries.single() as EmulatorSystem

        assertEquals(SaveTargetType.DIRECTORY, loaded.saveTargets.single().type)
        assertEquals("global", loaded.saveTargets.single().cloudPath)
        assertEquals(SaveTargetType.FILE, loaded.saveTargetsByRom.getValue("Game.rom").single().type)
        assertEquals("profiles/main/save.dat", loaded.saveTargetsByRom.getValue("Game.rom").single().cloudPath)
        assertEquals(loaded, manager.load().entries.single())
    }

    @Test
    fun `loading legacy directories keeps their original flat cloud layout`(@TempDir directory: File) {
        val first = File(directory, "first").apply { mkdirs() }
        val second = File(directory, "second").apply { mkdirs() }
        val manager = ConfigManager(File(directory, "config.json"))
        manager.save(AppConfig(entries = listOf(entry().copy(savePaths = listOf(first.path, second.path)))))

        val loaded = manager.load().entries.single()

        assertEquals(listOf(first.path, second.path), loaded.saveTargets.map { it.path })
        assertTrue(loaded.saveTargets.all { it.cloudPath == null })
    }

    @Test
    fun `ROM mapping with a different cloud prefix is distinct from the global fallback`(@TempDir directory: File) {
        SaveMappingDrive().use { drive ->
            val path = File(directory, "same-local-root").path
            val system = EmulatorSystem(
                "Console", "echo", romsDirectory = directory.path, extensions = listOf("rom"),
                saveTargets = listOf(SaveTarget(path, SaveTargetType.DIRECTORY, "global")),
                saveTargetsByRom = mapOf("Game.rom" to listOf(SaveTarget(path, SaveTargetType.DIRECTORY, "game-specific"))),
            )
            val orchestrator = SyncOrchestrator(drive.client, AppConfig(entries = listOf(system)), ConfigManager(File(directory, "config.json"))) {}

            assertEquals(listOf("Console", "Game"), orchestrator.getCloudPathSegments(system, "Game.rom"))
            assertEquals(setOf("global", "game-specific"), orchestrator.getSaveTargets(system).map { it.cloudPath }.toSet())
        }
    }

    @Test
    fun `individual nested restore keeps exact keys before play and when uploading`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            drive.add("one", "profiles/slot1/save.dat", "SLOT ONE")
            drive.add("two", "profiles/slot2/save.dat", "SLOT TWO")
            val destination = File(directory, "restored")
            val first = File(destination, "profiles/slot1/save.dat")
            val second = File(destination, "profiles/slot2/save.dat")
            val statuses = mutableListOf<AppStatus>()
            var restoredTargets = emptyList<SaveTarget>()
            val entry = entry()
            val orchestrator = orchestrator(drive, directory, entry) { status ->
                statuses += status
                if (status == AppStatus.Playing) {
                    assertEquals("SLOT ONE", first.readText())
                    assertEquals("SLOT TWO", second.readText())
                    change(first, "UPDATED SLOT ONE")
                }
            }

            orchestrator.playWithSync(GameItem("Game", entry), onRequestSaveSetup = { _, _, names ->
                resolveCloudRestoreTargets(destination.path, names).also { restoredTargets = it }
            })

            assertEquals(listOf("profiles/slot1/save.dat", "profiles/slot2/save.dat"), restoredTargets.map { it.cloudPath })
            assertEquals(listOf("profiles/slot1/save.dat"), drive.uploadedNames)
            assertEquals("UPDATED SLOT ONE", drive.contents.getValue("one"))
            assertEquals("SLOT TWO", drive.contents.getValue("two"))
            assertFalse(File(destination, "save.dat").exists())
            assertEquals(AppStatus.Idle, statuses.last())
        }
    }

    @Test
    fun `two missing directory roots restore identical relative names to their own destinations`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            drive.add("progress", "progress/slot/save.dat", "PROGRESS")
            drive.add("trophies", "trophies/slot/save.dat", "TROPHIES")
            drive.add("other", "unmanaged/other.dat", "UNRELATED")
            val targets = listOf(
                SaveTarget(File(directory, "new-progress").path, SaveTargetType.DIRECTORY, "progress"),
                SaveTarget(File(directory, "new-trophies").path, SaveTargetType.DIRECTORY, "trophies"),
            )
            val entry = entry(targets)
            val orchestrator = orchestrator(drive, directory, entry)

            assertEquals(CloudSyncStatus.OUT_OF_SYNC, orchestrator.checkEntrySyncStatus(entry))
            assertTrue(orchestrator.syncEntry(entry))

            assertEquals("PROGRESS", File(targets[0].path, "slot/save.dat").readText())
            assertEquals("TROPHIES", File(targets[1].path, "slot/save.dat").readText())
            assertFalse(File(targets[0].path, "progress").exists())
            assertFalse(File(targets[1].path, "unmanaged").exists())
            assertEquals(setOf("progress", "trophies"), drive.downloads.toSet())
            assertTrue(drive.uploadedNames.isEmpty())
            assertEquals(CloudSyncStatus.IN_SYNC, orchestrator.checkEntrySyncStatus(entry))
        }
    }

    @Test
    fun `saved mapping survives changing local paths and reversing directory order`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            drive.add("progress", "progress/save.dat", "PROGRESS")
            drive.add("trophies", "trophies/save.dat", "TROPHIES")
            val manager = ConfigManager(File(directory, "config.json"))
            val original = entry(listOf(
                SaveTarget("/old/device/progress", SaveTargetType.DIRECTORY, "progress"),
                SaveTarget("/old/device/trophies", SaveTargetType.DIRECTORY, "trophies"),
            ))
            manager.save(configuration(original))
            val loaded = manager.load().entries.single() as NativePCGame
            val relocated = loaded.copy(saveTargets = loaded.saveTargets.reversed().map {
                it.copy(path = File(directory, "another-device/${it.cloudPath}").path)
            })
            manager.save(configuration(relocated))

            assertTrue(orchestrator(drive, directory, relocated).syncEntry(relocated))

            assertEquals("PROGRESS", File(directory, "another-device/progress/save.dat").readText())
            assertEquals("TROPHIES", File(directory, "another-device/trophies/save.dat").readText())
        }
    }

    @Test
    fun `manual upload updates only the mapped directory even when relative names match`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            val targets = scopedTargets(directory)
            drive.add("progress", "progress/save.dat", "OLD PROGRESS")
            drive.add("trophies", "trophies/save.dat", "TROPHIES")
            seed(File(targets[0].path, "save.dat"), "NEW PROGRESS", newer = true)
            seed(File(targets[1].path, "save.dat"), "TROPHIES")
            val entry = entry(targets)

            assertTrue(orchestrator(drive, directory, entry).syncEntry(entry))

            assertEquals(listOf("progress/save.dat"), drive.uploadedNames)
            assertEquals("NEW PROGRESS", drive.contents.getValue("progress"))
            assertEquals("TROPHIES", drive.contents.getValue("trophies"))
            assertEquals(2, drive.files.size)
        }
    }

    @Test
    fun `pre-play and post-play use the same directory mappings`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            val targets = scopedTargets(directory)
            drive.add("progress", "progress/save.dat", "PROGRESS")
            drive.add("trophies", "trophies/save.dat", "TROPHIES")
            val entry = entry(targets)
            var played = false
            val orchestrator = orchestrator(drive, directory, entry) { status ->
                if (status == AppStatus.Playing) {
                    played = true
                    assertEquals("PROGRESS", File(targets[0].path, "save.dat").readText())
                    assertEquals("TROPHIES", File(targets[1].path, "save.dat").readText())
                    change(File(targets[0].path, "save.dat"), "NEW PROGRESS")
                    File(targets[0].path, "new/slot.dat").apply { parentFile.mkdirs(); writeText("NEW SLOT") }
                }
            }

            orchestrator.playWithSync(GameItem("Game", entry))

            assertTrue(played)
            assertEquals(setOf("progress/save.dat", "progress/new/slot.dat"), drive.uploadedNames.toSet())
            assertEquals("TROPHIES", drive.contents.getValue("trophies"))
        }
    }

    @Test
    fun `partial upload retry retains the full remote path and does not repeat completed saves`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            val targets = scopedTargets(directory)
            drive.add("progress", "progress/save.dat", "PROGRESS")
            drive.add("trophies", "trophies/save.dat", "TROPHIES")
            drive.failOnce += "progress/save.dat"
            val entry = entry(targets)
            val statuses = mutableListOf<AppStatus>()
            val orchestrator = orchestrator(drive, directory, entry) { status ->
                statuses += status
                if (status == AppStatus.Playing) {
                    change(File(targets[0].path, "save.dat"), "NEW PROGRESS")
                    change(File(targets[1].path, "save.dat"), "NEW TROPHIES")
                }
            }

            orchestrator.playWithSync(GameItem("Game", entry))
            val failure = statuses.last() as AppStatus.PostSyncFailed
            assertEquals(1, failure.pendingSaveCount)
            failure.onRetry()

            assertEquals(AppStatus.Idle, statuses.last())
            assertEquals(listOf("progress/save.dat", "trophies/save.dat", "progress/save.dat"), drive.startedNames)
            assertEquals("NEW PROGRESS", drive.contents.getValue("progress"))
            assertEquals("NEW TROPHIES", drive.contents.getValue("trophies"))
            assertEquals(2, drive.files.size)
        }
    }

    @Test
    fun `ambiguous legacy restore fails before writing files or launching`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            drive.add("first", "progress.dat", "PROGRESS")
            drive.add("second", "trophies.dat", "TROPHIES")
            val targets = listOf(
                SaveTarget(File(directory, "missing-one").path, SaveTargetType.DIRECTORY),
                SaveTarget(File(directory, "missing-two").path, SaveTargetType.DIRECTORY),
            )
            val entry = entry(targets)
            val statuses = mutableListOf<AppStatus>()
            val orchestrator = orchestrator(drive, directory, entry) { statuses += it }

            assertFalse(orchestrator.syncEntry(entry))
            assertEquals(CloudSyncStatus.ERROR, orchestrator.checkEntrySyncStatus(entry))
            orchestrator.playWithSync(GameItem("Game", entry))

            assertTrue(statuses.last() is AppStatus.Error)
            assertFalse(AppStatus.Playing in statuses)
            assertTrue(targets.none { File(it.path).exists() })
            assertTrue(drive.downloads.isEmpty())
            assertTrue(drive.startedNames.isEmpty())
        }
    }

    @Test
    fun `legacy roots with colliding local names are not selected by timestamps`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            val first = File(directory, "first").apply { mkdirs() }
            val second = File(directory, "second").apply { mkdirs() }
            seed(File(first, "save.dat"), "FIRST")
            seed(File(second, "save.dat"), "SECOND", newer = true)
            val entry = entry(listOf(SaveTarget(first.path, SaveTargetType.DIRECTORY), SaveTarget(second.path, SaveTargetType.DIRECTORY)))

            assertFalse(orchestrator(drive, directory, entry).syncEntry(entry))

            assertEquals("FIRST", File(first, "save.dat").readText())
            assertEquals("SECOND", File(second, "save.dat").readText())
            assertTrue(drive.startedNames.isEmpty())
        }
    }

    @Test
    fun `legacy roots do not claim newly discovered unassigned remote files`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            val first = File(directory, "first").apply { mkdirs() }
            val second = File(directory, "second").apply { mkdirs() }
            seed(File(first, "first.dat"), "FIRST")
            seed(File(second, "second.dat"), "SECOND")
            drive.add("unassigned", "new.dat", "FROM ANOTHER DEVICE")
            val entry = entry(listOf(SaveTarget(first.path, SaveTargetType.DIRECTORY), SaveTarget(second.path, SaveTargetType.DIRECTORY)))

            assertFalse(orchestrator(drive, directory, entry).syncEntry(entry))
            assertFalse(File(first, "new.dat").exists())
            assertFalse(File(second, "new.dat").exists())
            assertTrue(drive.downloads.isEmpty())
        }
    }

    @Test
    fun `file mapping survives a different local filename`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            drive.add("remote", "profiles/main/save.dat", "REMOTE")
            val file = File(directory, "local-name.sav")
            val entry = entry(listOf(SaveTarget(file.path, SaveTargetType.FILE, "profiles/main/save.dat")))
            val orchestrator = orchestrator(drive, directory, entry)

            assertTrue(orchestrator.syncEntry(entry))
            assertEquals("REMOTE", file.readText())
            change(file, "LOCAL UPDATE")
            assertTrue(orchestrator.syncEntry(entry))
            assertEquals(listOf("profiles/main/save.dat"), drive.uploadedNames)
            assertEquals(1, drive.files.size)
        }
    }

    @Test
    fun `overlapping directory mappings fail before any transfer`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            drive.add("remote", "progress/slots/save.dat", "REMOTE")
            val entry = entry(listOf(
                SaveTarget(File(directory, "a").path, SaveTargetType.DIRECTORY, "progress"),
                SaveTarget(File(directory, "b").path, SaveTargetType.DIRECTORY, "progress/slots"),
            ))

            assertFalse(orchestrator(drive, directory, entry).syncEntry(entry))
            assertTrue(drive.downloads.isEmpty())
        }
    }

    @Test
    fun `invalid cloud path does not silently fall back to playing offline`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            val entry = entry(listOf(SaveTarget(File(directory, "save.dat").path, SaveTargetType.FILE, "../escape.dat")))
            val statuses = mutableListOf<AppStatus>()

            orchestrator(drive, directory, entry) { statuses += it }.playWithSync(GameItem("Game", entry))

            assertTrue(statuses.last() is AppStatus.Error)
            assertFalse(AppStatus.Playing in statuses)
            assertTrue(drive.downloads.isEmpty())
        }
    }

    @Test
    fun `new directory scopes are distinct even with the same basename and explicit prefixes`() {
        val targets = listOf(
            SaveTarget("/device/a/SAVE", SaveTargetType.DIRECTORY),
            SaveTarget("/device/b/SAVE", SaveTargetType.DIRECTORY),
            SaveTarget("/device/c", SaveTargetType.DIRECTORY, "save-target-1/nested"),
        )
        val scoped = scopeNewSaveTargets(targets)

        assertEquals(listOf("save-target-2", "save-target-3", "save-target-1/nested"), scoped.map { it.cloudPath })
        assertEquals(scoped, scopeNewSaveTargets(scoped))
        assertEquals(targets.map { it.path }, scoped.map { it.path })
    }

    private fun entry(targets: List<SaveTarget> = emptyList()) = NativePCGame(
        name = "Game",
        executablePath = File(System.getProperty("java.home"), "bin/${if (System.getProperty("os.name").lowercase().contains("win")) "java.exe" else "java"}").path,
        arguments = listOf("-version"),
        saveTargets = targets,
    )

    private fun scopedTargets(directory: File) = listOf(
        SaveTarget(File(directory, "progress").path, SaveTargetType.DIRECTORY, "progress"),
        SaveTarget(File(directory, "trophies").path, SaveTargetType.DIRECTORY, "trophies"),
    )

    private fun configuration(entry: NativePCGame) = AppConfig(GoogleDriveConfig("client", "secret", "refresh"), listOf(entry))

    private fun orchestrator(drive: SaveMappingDrive, directory: File, entry: NativePCGame, onStatus: (AppStatus) -> Unit = {}): SyncOrchestrator =
        SyncOrchestrator(drive.client, configuration(entry), ConfigManager(File(directory, "config.json"))) {
            if (it is AppStatus.Conflict) it.onResolve(SyncDecision.UPLOAD_LOCAL)
            onStatus(it)
        }

    private fun seed(file: File, content: String, newer: Boolean = false) {
        file.parentFile.mkdirs()
        file.writeText(content)
        file.setLastModified(Instant.parse(if (newer) "2026-10-02T12:00:00Z" else SaveMappingDrive.CLOUD_TIME).toEpochMilli())
    }

    private fun change(file: File, content: String) {
        file.writeText(content)
        file.setLastModified(Instant.now().plusSeconds(60).toEpochMilli())
    }
}
