package com.emusync.scanner

import java.io.File

/**
 * Data class representing a discovered ROM file.
 *
 * @property file The ROM [File] on disk.
 * @property name Display-friendly name (filename without extension).
 * @property extension The file extension (lowercase, without dot).
 */
data class RomFile(
    val file: File,
    val name: String,
    val extension: String,
)
