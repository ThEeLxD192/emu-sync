package com.emusync.ui

import com.emusync.ui.components.loadGameCover
import com.emusync.ui.components.loadGameCoverAsync
import java.awt.image.BufferedImage
import java.io.File
import javax.imageio.ImageIO
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNotSame
import kotlin.test.assertNull
import kotlin.test.assertSame
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
