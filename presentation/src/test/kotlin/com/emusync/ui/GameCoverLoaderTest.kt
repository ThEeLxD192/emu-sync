package com.emusync.ui

import com.emusync.domain.library.CoverFiles
import com.emusync.domain.library.NativePCGame
import com.emusync.ui.library.GameItem
import com.emusync.ui.library.components.loadGameCover
import com.emusync.ui.library.components.loadGameCoverAsync
import java.awt.image.BufferedImage
import java.io.File
import java.util.concurrent.Executors
import javax.imageio.ImageIO
import kotlin.test.assertEquals
import kotlin.test.assertNotEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNotSame
import kotlin.test.assertNull
import kotlin.test.assertSame
import kotlin.test.assertTrue
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class GameCoverLoaderTest {
    @Test
    fun `async cover loading uses the same cache with an injected dispatcher`(
        @TempDir directory: File
    ) = runTest {
        val file = File(directory, "cover.png")
        ImageIO.write(BufferedImage(16, 16, BufferedImage.TYPE_INT_ARGB), "png", file)
        val cached = assertNotNull(loadGameCover(file))

        assertSame(cached, loadGameCoverAsync(file, StandardTestDispatcher(testScheduler)))
    }

    @Test
    fun `grid and preview requests resolve and decode covers on the IO dispatcher`(
        @TempDir directory: File
    ) = runTest {
        val caller = Thread.currentThread()
        val file = File(directory, "off-thread.png")
        ImageIO.write(BufferedImage(23, 17, BufferedImage.TYPE_INT_ARGB), "png", file)
        val game = GameItem("Game", NativePCGame("Game", "game.exe"))
        var lookups = 0
        val covers = CoverFiles { name, entry, romPath ->
            assertNotEquals(caller, Thread.currentThread())
            assertEquals("cover-worker", Thread.currentThread().name)
            assertEquals("Game", name)
            assertEquals(game.entry, entry)
            assertNull(romPath)
            lookups++
            file.path
        }
        Executors.newSingleThreadExecutor { Thread(it, "cover-worker") }
            .asCoroutineDispatcher()
            .use { io ->
                val grid = loadGameCoverAsync(game, covers, ioDispatcher = io)
                assertEquals(file.path, grid.path)
                assertEquals(23, assertNotNull(grid.bitmap).width)
                val preview = loadGameCoverAsync(game, covers, file.path, io)
                assertSame(grid.bitmap, preview.bitmap)
                val missing =
                    loadGameCoverAsync(game, covers, File(directory, "missing.png").path, io)
                assertNull(missing.path)
                assertNull(missing.bitmap)
            }
        assertEquals(1, lookups)
        assertTrue(Thread.currentThread() === caller)
    }

    @Test
    fun `replacing cover content at the same path invalidates the cached bitmap`(
        @TempDir directory: File
    ) {
        val file = File(directory, "cover.png")
        ImageIO.write(BufferedImage(16, 16, BufferedImage.TYPE_INT_ARGB), "png", file)
        val first = assertNotNull(loadGameCover(file))
        assertEquals(16, first.width)
        assertSame(first, loadGameCover(file))
        ImageIO.write(BufferedImage(32, 24, BufferedImage.TYPE_INT_ARGB), "png", file)
        file.setLastModified(System.currentTimeMillis() + 2_000)
        val second = assertNotNull(loadGameCover(file))
        assertEquals(32, second.width)
        assertEquals(24, second.height)
        assertNotSame(first, second)
    }

    @Test
    fun `missing directory and corrupt covers fall back without crashing`(
        @TempDir directory: File
    ) {
        assertNull(loadGameCover(null))
        assertNull(loadGameCover(directory))
        assertNull(loadGameCover(File(directory, "missing.png")))
        val bad = File(directory, "bad.png").apply { writeText("invalid image") }
        assertNull(loadGameCover(bad))
    }
}
