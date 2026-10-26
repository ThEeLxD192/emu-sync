package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.model.AppConfig
import com.emusync.model.GoogleDriveConfig
import com.emusync.model.NativePCGame
import com.emusync.model.SaveTarget
import com.emusync.model.SaveTargetType
import com.emusync.ui.AppStatus
import com.emusync.ui.CloudSyncStatus
import com.emusync.ui.GameItem
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import java.io.IOException
import java.nio.file.Files
import java.nio.file.attribute.PosixFilePermissions
import java.time.Instant
import kotlin.test.*

class TrackedFileSyncTest {
    private val time = SaveMappingDrive.CLOUD_TIME
    private val original = sha256("ORIGINAL".toByteArray())
    private val localChange = sha256("LOCAL".toByteArray())
    private val cloudChange = sha256("CLOUD".toByteArray())
    private fun remote(hash: String? = original, version: String? = "1", date: String = time) =
        DriveFile("remote", "save.dat", date, sha256Checksum = hash, version = version)
    private fun previous() = SyncedSave(original, "remote", time, original, "1")

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
        assertEquals(SyncDecision.DOWNLOAD_CLOUD, planTrackedSync(original, remote(cloudChange), previous()))
        assertEquals(SyncDecision.CONFLICT, planTrackedSync(localChange, remote(cloudChange), previous()))
    }

    @Test
    fun `independent changes to the same content converge without a conflict`() {
        assertEquals(SyncDecision.IN_SYNC, planTrackedSync(localChange, remote(localChange), previous()))
    }

    @Test
    fun `metadata version is used when checksum is absent`() {
        assertEquals(SyncDecision.DOWNLOAD_CLOUD, planTrackedSync(original, remote(null, "2"), previous()))
        assertEquals(SyncDecision.CONFLICT, planTrackedSync(localChange, remote(null, "2"), previous()))
    }

    @Test
    fun `legacy remote metadata compares exact timestamps without tolerance`() {
        val checkpoint = previous().copy(remoteSha256 = null, remoteVersion = null)
        assertEquals(SyncDecision.IN_SYNC, planTrackedSync(original, remote(null, null), checkpoint))
        assertEquals(SyncDecision.DOWNLOAD_CLOUD, planTrackedSync(original,
            remote(null, null, "2026-10-01T12:00:00.001Z"), checkpoint))
    }

    @Test
    fun `remote replacement with a different id is a change`() {
        assertEquals(SyncDecision.CONFLICT, planTrackedSync(localChange,
            remote(null).copy(id = "replacement"), previous()))
    }

    @Test
    fun `one missing copy is restored without propagating deletions`() {
        assertEquals(SyncDecision.UPLOAD_LOCAL, planTrackedSync(original, null, previous()))
        assertEquals(SyncDecision.DOWNLOAD_CLOUD, planTrackedSync(null, remote(), previous()))
        assertEquals(SyncDecision.IN_SYNC, planTrackedSync(null, null, previous()))
    }

    @Test
    fun `history survives restart and merges records from independent instances`(@TempDir directory: File) {
        val config = File(directory, "config.json")
        val a = SyncHistoryStore(config)
        val b = SyncHistoryStore(config)
        val local = File(directory, "save.dat")
        val key = a.key(local, "folder", "prefix/save.dat")
        a.record(key, original, remote())
        b.record("other", localChange, remote(localChange))
        assertEquals(previous(), SyncHistoryStore(config).get(key))
        assertEquals(localChange, a.get("other")?.localSha256)
        assertNotEquals(key, a.key(local, "other-folder", "prefix/save.dat"))
        assertNotEquals(key, a.key(local, "folder", "other/save.dat"))
        assertNotEquals(key, a.key(File(directory, "elsewhere/save.dat"), "folder", "prefix/save.dat"))
        assertEquals(key, a.key(File(directory, "nested/../save.dat"), "folder", "prefix/save.dat"))
        assertFalse(a.stateFile.readText().contains(directory.absolutePath))
        if (Files.getFileStore(directory.toPath()).supportsFileAttributeView("posix")) {
            assertEquals(PosixFilePermissions.fromString("rw-------"), Files.getPosixFilePermissions(a.stateFile.toPath()))
        }
    }

    @Test
    fun `failed history replacement retains the previous complete index`(@TempDir directory: File) {
        val config = File(directory, "config.json")
        val normal = SyncHistoryStore(config)
        normal.record("save", original, remote())
        val before = normal.stateFile.readText()
        val failing = SyncHistoryStore(config) { throw IOException("simulated disk failure") }
        assertFailsWith<SaveSyncException> { failing.record("save", localChange, remote(localChange)) }
        assertEquals(before, normal.stateFile.readText())
        assertEquals(original, normal.get("save")?.localSha256)
        assertEquals(listOf(normal.stateFile.name), directory.listFiles()!!.map { it.name })
    }

    @Test
    fun `corrupt or unsupported history fails safely and is never reset silently`(@TempDir directory: File) {
        val history = SyncHistoryStore(File(directory, "config.json"))
        for (contents in listOf("broken JSON", """{"formatVersion":2,"saves":{}}""")) {
            history.stateFile.writeText(contents)
            assertFailsWith<SaveSyncException> { history.get("save") }
            assertFailsWith<SaveSyncException> { history.record("save", original, remote()) }
            assertEquals(contents, history.stateFile.readText())
        }
    }

    @Test
    fun `download preserves a deduplicated recovery copy and advances history`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            drive.add("remote", "save.dat", "CLOUD")
            val local = File(directory, "save.dat").apply { writeText("LOCAL") }
            val history = SyncHistoryStore(File(directory, "config.json"))
            val tracker = TrackedFileSync(DriveTransfer(drive.client), history)
            tracker.download("token", drive.files.getValue("remote"), local, "game", "save.dat")
            assertEquals("CLOUD", local.readText())
            assertEquals(cloudChange, history.get(history.key(local, "game", "save.dat"))?.localSha256)
            local.writeText("LOCAL")
            tracker.download("token", drive.files.getValue("remote"), local, "game", "save.dat")
            val backups = history.backupDirectory.walkTopDown().filter { it.isFile && it.name == "save.dat" }.toList()
            assertEquals(1, backups.size)
            assertEquals("LOCAL", backups.single().readText())
            assertEquals(Instant.parse(time).toEpochMilli(), local.lastModified())
            assertFalse(directory.listFiles()!!.any { it.name.endsWith(".restore") || it.name.endsWith(".part") })
        }
    }

    @Test
    fun `backup failure prevents replacement`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            drive.add("remote", "save.dat", "CLOUD")
            val local = File(directory, "save.dat").apply { writeText("LOCAL") }
            val history = SyncHistoryStore(File(directory, "config.json"))
            history.backupDirectory.writeText("cannot create a directory here")
            assertFailsWith<SaveSyncException> {
                TrackedFileSync(DriveTransfer(drive.client), history).download("token", drive.files.getValue("remote"), local, "game", "save.dat")
            }
            assertEquals("LOCAL", local.readText())
            assertNull(history.get(history.key(local, "game", "save.dat")))
        }
    }

    @Test
    fun `checksum mismatch and cancelled downloads keep local data and history`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            drive.add("remote", "save.dat", "CLOUD")
            val local = File(directory, "save.dat").apply { writeText("LOCAL") }
            val history = SyncHistoryStore(File(directory, "config.json"))
            val tracker = TrackedFileSync(DriveTransfer(drive.client), history)
            val metadata = drive.files.getValue("remote")
            drive.contents["remote"] = "UNEXPECTED"
            assertFailsWith<SaveSyncException> { tracker.download("token", metadata, local, "game", "save.dat") }
            drive.beforeDownload = { throw CancellationException("cancelled") }
            assertFailsWith<CancellationException> { tracker.download("token", metadata, local, "game", "save.dat") }
            assertEquals("LOCAL", local.readText())
            assertNull(history.get(history.key(local, "game", "save.dat")))
            assertFalse(directory.listFiles()!!.any { it.name.endsWith(".restore") || it.name.endsWith(".part") })
        }
    }

    @Test
    fun `local progress created during download is retained`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            drive.add("remote", "save.dat", "CLOUD")
            val local = File(directory, "save.dat").apply { writeText("LOCAL") }
            val history = SyncHistoryStore(File(directory, "config.json"))
            drive.beforeDownload = { local.writeText("NEW LOCAL") }
            assertFailsWith<SaveSyncException> {
                TrackedFileSync(DriveTransfer(drive.client), history).download("token", drive.files.getValue("remote"), local, "game", "save.dat")
            }
            assertEquals("NEW LOCAL", local.readText())
            assertNull(history.get(history.key(local, "game", "save.dat")))
        }
    }

    @Test
    fun `upload uses a stable snapshot and newer local progress remains pending`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            val local = File(directory, "save.dat").apply { writeText("LOCAL") }
            val history = SyncHistoryStore(File(directory, "config.json"))
            val tracker = TrackedFileSync(DriveTransfer(drive.client), history)
            drive.beforeUpload = { local.writeText("NEW LOCAL") }
            assertFailsWith<SaveSyncException> { tracker.upload("token", local, null, "game", "save.dat") }
            assertEquals("LOCAL", drive.contents.values.single())
            assertEquals("NEW LOCAL", local.readText())
            assertEquals(localChange, history.get(history.key(local, "game", "save.dat"))?.localSha256)
            assertEquals(SyncDecision.UPLOAD_LOCAL, tracker.plan(local, drive.files.values.single(), "game", "save.dat"))
        }
    }

    @Test
    fun `failed upload leaves previous history and local progress intact`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            drive.add("remote", "save.dat", "ORIGINAL")
            val local = File(directory, "save.dat").apply { writeText("LOCAL") }
            val history = SyncHistoryStore(File(directory, "config.json"))
            val key = history.key(local, "game", "save.dat")
            history.record(key, original, drive.files.getValue("remote"))
            drive.failOnce += "save.dat"
            assertFailsWith<DriveApiException> {
                TrackedFileSync(DriveTransfer(drive.client), history).upload("token", local, drive.files.getValue("remote"), "game", "save.dat")
            }
            assertEquals(original, history.get(key)?.localSha256)
            assertEquals("LOCAL", local.readText())
            assertEquals("ORIGINAL", drive.contents.getValue("remote"))
        }
    }

    @Test
    fun `two devices changing offline conflict after restart even with identical dates`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            drive.add("remote", "save.dat", "ORIGINAL")
            val first = File(directory, "device-one/save.dat").apply { parentFile.mkdirs(); writeText("ORIGINAL") }
            val second = File(directory, "device-two/save.dat").apply { parentFile.mkdirs(); writeText("ORIGINAL") }
            val entryA = game(first)
            val entryB = game(second)
            val configA = File(directory, "device-one/config.json")
            val configB = File(directory, "device-two/config.json")
            assertTrue(orchestrator(drive, configA, entryA).syncEntry(entryA))
            assertTrue(orchestrator(drive, configB, entryB).syncEntry(entryB))
            first.writeText("LOCAL")
            second.writeText("CLOUD")
            assertTrue(orchestrator(drive, configB, entryB).syncEntry(entryB))
            drive.files["remote"] = drive.files.getValue("remote").copy(modifiedTime = time)
            first.setLastModified(Instant.parse(time).toEpochMilli())
            var conflicts = 0
            val restarted = orchestrator(drive, configA, entryA) { status ->
                if (status is AppStatus.Conflict) {
                    conflicts++
                    assertEquals("LOCAL", first.readText())
                    assertEquals("CLOUD", drive.contents.getValue("remote"))
                    status.onResolve(SyncDecision.DOWNLOAD_CLOUD)
                }
            }
            assertEquals(CloudSyncStatus.CONFLICT, restarted.checkEntrySyncStatus(entryA))
            assertTrue(restarted.syncEntry(entryA))
            assertEquals(1, conflicts)
            assertEquals("CLOUD", first.readText())
            val history = SyncHistoryStore(configA)
            assertEquals("LOCAL", history.backupDirectory.walkTopDown().single { it.isFile && it.name == "save.dat" }.readText())
            assertEquals(CloudSyncStatus.IN_SYNC, orchestrator(drive, configA, entryA).checkEntrySyncStatus(entryA))
        }
    }

    @Test
    fun `status inspection does not establish synchronization history`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            drive.add("remote", "save.dat", "ORIGINAL")
            val local = File(directory, "save.dat").apply { writeText("ORIGINAL") }
            val config = File(directory, "config.json")
            val entry = game(local)
            assertEquals(CloudSyncStatus.IN_SYNC, orchestrator(drive, config, entry).checkEntrySyncStatus(entry))
            assertFalse(SyncHistoryStore(config).stateFile.exists())
            assertTrue(drive.downloads.isEmpty())
            assertTrue(drive.uploadedNames.isEmpty())
        }
    }

    @Test
    fun `pre-play local choice does not force unrelated post-play conflicts`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            drive.add("first", "first.dat", "ORIGINAL")
            drive.add("second", "second.dat", "ORIGINAL")
            val first = File(directory, "first.dat").apply { writeText("ORIGINAL") }
            val second = File(directory, "second.dat").apply { writeText("ORIGINAL") }
            val entry = game(first).copy(saveTargets = listOf(first, second).map { SaveTarget(it.path, SaveTargetType.FILE) })
            val config = File(directory, "config.json")
            assertTrue(orchestrator(drive, config, entry).syncEntry(entry))
            first.writeText("LOCAL")
            drive.add("first", "first.dat", "CLOUD")
            var conflicts = 0
            val statuses = mutableListOf<AppStatus>()
            orchestrator(drive, config, entry) { status ->
                statuses += status
                if (status is AppStatus.Conflict) {
                    conflicts++
                    status.onResolve(if (conflicts == 1) SyncDecision.UPLOAD_LOCAL else SyncDecision.DOWNLOAD_CLOUD)
                }
                if (status == AppStatus.Playing) {
                    assertEquals("LOCAL", drive.contents.getValue("first"), "Explicit local choice must finish before launch")
                    second.writeText("LOCAL")
                    drive.add("second", "second.dat", "CLOUD")
                }
            }.playWithSync(GameItem(entry.name, entry))
            assertEquals(2, conflicts)
            assertEquals(AppStatus.Idle, statuses.last())
            assertEquals("CLOUD", second.readText())
            assertEquals("CLOUD", drive.contents.getValue("second"))
            assertEquals(listOf("first.dat"), drive.uploadedNames)
        }
    }

    @Test
    fun `corrupt history aborts launch instead of falling back to offline play`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            val local = File(directory, "save.dat").apply { writeText("ORIGINAL") }
            val entry = game(local)
            val config = File(directory, "config.json")
            SyncHistoryStore(config).stateFile.writeText("broken")
            val statuses = mutableListOf<AppStatus>()
            orchestrator(drive, config, entry, statuses::add).playWithSync(GameItem(entry.name, entry))
            assertFalse(statuses.contains(AppStatus.Playing))
            assertIs<AppStatus.Error>(statuses.last())
            assertEquals("ORIGINAL", local.readText())
        }
    }

    @Test
    fun `failed explicit local resolution aborts launch and leaves both copies intact`(@TempDir directory: File) = runTest {
        SaveMappingDrive().use { drive ->
            drive.add("remote", "save.dat", "CLOUD")
            val local = File(directory, "save.dat").apply { writeText("LOCAL") }
            val entry = game(local)
            drive.failOnce += "save.dat"
            val statuses = mutableListOf<AppStatus>()
            orchestrator(drive, File(directory, "config.json"), entry) {
                statuses += it
                if (it is AppStatus.Conflict) it.onResolve(SyncDecision.UPLOAD_LOCAL)
            }.playWithSync(GameItem(entry.name, entry))
            assertIs<AppStatus.Error>(statuses.last())
            assertFalse(statuses.contains(AppStatus.Playing))
            assertEquals("LOCAL", local.readText())
            assertEquals("CLOUD", drive.contents.getValue("remote"))
        }
    }

    private fun game(file: File) = NativePCGame("Game", executablePath = "echo",
        saveTargets = listOf(SaveTarget(file.path, SaveTargetType.FILE)))

    private fun orchestrator(drive: SaveMappingDrive, configFile: File, entry: NativePCGame,
        onStatus: (AppStatus) -> Unit = { if (it is AppStatus.Conflict) fail("Unexpected conflict") }) =
        SyncOrchestrator(drive.client, AppConfig(GoogleDriveConfig("client", "secret", "refresh"), listOf(entry)),
            ConfigManager(configFile), onStatus)
}
