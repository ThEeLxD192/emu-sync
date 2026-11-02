package com.emusync.config

import com.emusync.domain.config.AppConfig
import com.emusync.domain.config.OAuthCredentials
import com.emusync.domain.library.LibraryStore
import com.emusync.domain.library.NativePCGame
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class ConfigUpdatesTest {
    @Test
    fun `updates from separate manager instances preserve concurrent additions`(
        @TempDir directory: File
    ) {
        val first = ConfigManager(File(directory, "config.json"))
        val second = ConfigManager(File(directory, "./config.json"))
        val credentials = OAuthCredentials("client", "secret", "refresh")
        first.save(AppConfig(cloudCredentials = credentials))
        val start = CountDownLatch(1)
        val executor = Executors.newFixedThreadPool(2)
        try {
            val jobs =
                listOf<LibraryStore>(first, second).mapIndexed { index, manager ->
                    executor.submit {
                        check(start.await(5, TimeUnit.SECONDS))
                        repeat(10) { entry ->
                            manager.updateEntries { it + NativePCGame("$index-$entry", "echo") }
                        }
                    }
                }
            start.countDown()
            jobs.forEach { it.get(10, TimeUnit.SECONDS) }

            assertEquals(20, first.loadEntries().map { it.name }.toSet().size)
            assertEquals(credentials, first.load().cloudCredentials)
        } finally {
            executor.shutdownNow()
        }
    }

    @Test
    fun `explicit unlink invalidates an OAuth snapshot across manager instances`(
        @TempDir directory: File
    ) {
        val first = ConfigManager(File(directory, "config.json"))
        val second = ConfigManager(File(directory, "./config.json"))
        val config = AppConfig(OAuthCredentials("client", "secret"))
        first.save(config)
        val snapshot = first.snapshotCredentials(config)

        second.updateCredentials { it?.copy(refreshToken = null) }

        assertFalse(
            first.compareAndSetCredentials(
                snapshot,
                config.cloudCredentials!!.copy(refreshToken = "late-refresh"),
                config,
            )
        )
        assertEquals(config, first.load())
    }

    @Test
    fun `credential compare and set retains unrelated updated configuration`(
        @TempDir directory: File
    ) {
        val manager = ConfigManager(File(directory, "config.json"))
        val original = AppConfig(OAuthCredentials("client", "secret"))
        manager.save(original)
        val snapshot = manager.snapshotCredentials(original)
        val added = NativePCGame("Added", "echo")
        manager.update { it.copy(entries = listOf(added)) }

        assertTrue(
            manager.compareAndSetCredentials(
                snapshot,
                original.cloudCredentials!!.copy(refreshToken = "fresh"),
                original,
            )
        )
        assertEquals(listOf(added), manager.load().entries)
        assertEquals("fresh", manager.load().cloudCredentials?.refreshToken)
    }
}
