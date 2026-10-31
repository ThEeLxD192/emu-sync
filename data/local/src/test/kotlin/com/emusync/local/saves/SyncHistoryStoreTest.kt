package com.emusync.local.saves

import com.emusync.domain.sync.CloudSaveState
import com.emusync.domain.sync.SaveSyncException
import com.emusync.domain.sync.SyncCheckpoint
import java.io.File
import java.io.IOException
import java.nio.file.Files
import java.nio.file.attribute.PosixFilePermissions
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNotEquals
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class SyncHistoryStoreTest {
    private val time = "2026-10-01T12:00:00Z"
    private val original = sha256("ORIGINAL".toByteArray())
    private val localChange = sha256("LOCAL".toByteArray())

    private fun remote(hash: String? = original) = CloudSaveState("remote", time, hash, "1")

    private fun previous() = SyncCheckpoint(original, "remote", time, original, "1")

    @Test
    fun `history survives restart and merges records from independent instances`(
        @TempDir directory: File
    ) {
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
        assertNotEquals(
            key,
            a.key(File(directory, "elsewhere/save.dat"), "folder", "prefix/save.dat"),
        )
        assertEquals(key, a.key(File(directory, "nested/../save.dat"), "folder", "prefix/save.dat"))
        assertFalse(a.stateFile.readText().contains(directory.absolutePath))
        if (Files.getFileStore(directory.toPath()).supportsFileAttributeView("posix")) {
            assertEquals(
                PosixFilePermissions.fromString("rw-------"),
                Files.getPosixFilePermissions(a.stateFile.toPath()),
            )
        }
    }

    @Test
    fun `failed history replacement retains the previous complete index`(@TempDir directory: File) {
        val config = File(directory, "config.json")
        val normal = SyncHistoryStore(config)
        normal.record("save", original, remote())
        val before = normal.stateFile.readText()
        val failing = SyncHistoryStore(config) { throw IOException("simulated disk failure") }
        assertFailsWith<SaveSyncException> {
            failing.record("save", localChange, remote(localChange))
        }
        assertEquals(before, normal.stateFile.readText())
        assertEquals(original, normal.get("save")?.localSha256)
        assertEquals(listOf(normal.stateFile.name), directory.listFiles()!!.map { it.name })
    }

    @Test
    fun `corrupt or unsupported history fails safely and is never reset silently`(
        @TempDir directory: File
    ) {
        val history = SyncHistoryStore(File(directory, "config.json"))
        for (contents in listOf("broken JSON", """{"formatVersion":2,"saves":{}}""")) {
            history.stateFile.writeText(contents)
            assertFailsWith<SaveSyncException> { history.get("save") }
            assertFailsWith<SaveSyncException> { history.record("save", original, remote()) }
            assertEquals(contents, history.stateFile.readText())
        }
    }
}
