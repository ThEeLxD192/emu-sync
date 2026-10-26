package com.emusync.config

import com.emusync.model.AppConfig
import com.emusync.model.GoogleDriveConfig
import com.emusync.model.NativePCGame
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class ConfigUpdatesTest {
    @Test
    fun `updates from separate manager instances preserve concurrent additions`(@TempDir directory: File) {
        val first = ConfigManager(File(directory, "config.json"))
        val second = ConfigManager(File(directory, "./config.json"))
        val credentials = GoogleDriveConfig("client", "secret", "refresh")
        first.save(AppConfig(googleDrive = credentials))
        val start = CountDownLatch(1)
        val executor = Executors.newFixedThreadPool(2)
        try {
            val jobs = listOf(first, second).mapIndexed { index, manager ->
                executor.submit {
                    check(start.await(5, TimeUnit.SECONDS))
                    repeat(10) { entry ->
                        manager.update { it.copy(entries = it.entries + NativePCGame("$index-$entry", "echo")) }
                    }
                }
            }
            start.countDown()
            jobs.forEach { it.get(10, TimeUnit.SECONDS) }

            assertEquals(20, first.load().entries.map { it.name }.toSet().size)
            assertEquals(credentials, first.load().googleDrive)
        } finally { executor.shutdownNow() }
    }

    @Test
    fun `explicit unlink invalidates an OAuth snapshot across manager instances`(@TempDir directory: File) {
        val first = ConfigManager(File(directory, "config.json"))
        val second = ConfigManager(File(directory, "./config.json"))
        val config = AppConfig(GoogleDriveConfig("client", "secret"))
        first.save(config)
        val snapshot = first.snapshotGoogleDrive(config)

        second.updateGoogleDrive { it?.copy(refreshToken = null) }

        assertFalse(first.compareAndSetGoogleDrive(snapshot, config.googleDrive!!.copy(refreshToken = "late-refresh"), config))
        assertEquals(config, first.load())
    }

    @Test
    fun `credential compare and set retains unrelated updated configuration`(@TempDir directory: File) {
        val manager = ConfigManager(File(directory, "config.json"))
        val original = AppConfig(GoogleDriveConfig("client", "secret"))
        manager.save(original)
        val snapshot = manager.snapshotGoogleDrive(original)
        val added = NativePCGame("Added", "echo")
        manager.update { it.copy(entries = listOf(added)) }

        assertTrue(manager.compareAndSetGoogleDrive(snapshot, original.googleDrive!!.copy(refreshToken = "fresh"), original))
        assertEquals(listOf(added), manager.load().entries)
        assertEquals("fresh", manager.load().googleDrive?.refreshToken)
    }
}
