package com.emusync.ui.components

import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.toComposeImageBitmap
import org.jetbrains.skia.Image
import java.io.File
import java.util.Collections
import javax.imageio.ImageIO

private data class CoverCacheKey(val path: String, val length: Long, val modifiedTime: Long)

private val coverCache = Collections.synchronizedMap(
    object : LinkedHashMap<CoverCacheKey, ImageBitmap>(32, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<CoverCacheKey, ImageBitmap>?): Boolean {
            return size > 100
        }
    }
)

internal fun loadGameCover(file: File?): ImageBitmap? {
    if (file == null || !file.exists() || !file.isFile) return null
    val path = CoverCacheKey(file.absolutePath, file.length(), file.lastModified())
    coverCache[path]?.let { return it }
    return try {
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
