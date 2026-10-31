package com.emusync.ui.components

import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.toComposeImageBitmap
import java.io.File
import java.util.Collections
import javax.imageio.ImageIO
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.jetbrains.skia.Image

private data class CoverCacheKey(val path: String, val length: Long, val modifiedTime: Long)

private val coverCache =
    Collections.synchronizedMap(
        object : LinkedHashMap<CoverCacheKey, ImageBitmap>(32, 0.75f, true) {
            override fun removeEldestEntry(
                eldest: MutableMap.MutableEntry<CoverCacheKey, ImageBitmap>?
            ): Boolean {
                return size > 100
            }
        }
    )

internal fun loadGameCover(file: File?): ImageBitmap? {
    if (file == null || !file.exists() || !file.isFile) return null
    val path = CoverCacheKey(file.absolutePath, file.length(), file.lastModified())
    return coverCache[path]
        ?: try {
            val bytes = file.readBytes()
            val bitmap = Image.makeFromEncoded(bytes).toComposeImageBitmap()
            coverCache[path] = bitmap
            bitmap
        } catch (_: Exception) {
            try {
                val bitmap = ImageIO.read(file)?.toComposeImageBitmap()
                if (bitmap != null) coverCache[path] = bitmap
                bitmap
            } catch (_: Exception) {
                null
            }
        }
}

/** Keeps cover decoding off the Compose UI thread while allowing a caller-supplied dispatcher. */
internal suspend fun loadGameCoverAsync(
    file: File?,
    ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
): ImageBitmap? = withContext(ioDispatcher) { loadGameCover(file) }
