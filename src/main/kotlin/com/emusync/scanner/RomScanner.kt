package com.emusync.scanner

import java.io.File
import java.io.IOException
import java.nio.file.Path
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext

private val GAME_SIGNATURES = setOf("ps3_disc", "eboot", "app", "main")

/**
 * Scans a directory for ROM files matching the given extensions.
 *
 * Directory listing runs on [ioDispatcher], which defaults to [Dispatchers.IO], to prevent UI
 * freezes on large ROM collections.
 *
 * @param directory Absolute path to the directory to scan.
 * @param extensions List of allowed file extensions (case-insensitive, without dots). Example:
 *   `listOf("gba", "gbc", "zip")`
 * @param ioDispatcher Dispatcher used for filesystem operations.
 * @return Sorted list of [RomFile] objects found. Empty list if directory doesn't exist.
 */
suspend fun scanRoms(
    directory: String,
    extensions: List<String>,
    ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
): List<RomFile> =
    withContext(ioDispatcher) {
        val dir = File(directory)

        if (!dir.exists() || !dir.isDirectory) {
            return@withContext emptyList()
        }

        // Normalize extensions to lowercase
        val allowedExtensions =
            extensions
                .map { it.trim().removePrefix(".").lowercase() }
                .filter { it.isNotEmpty() }
                .toSet()
        if (allowedExtensions.isEmpty()) return@withContext emptyList()

        scanDirectories(dir, allowedExtensions)
            .distinctBy { it.file.absolutePath }
            .sortedBy { it.name.lowercase() }
    }

private suspend fun scanDirectories(root: File, allowedExtensions: Set<String>): List<RomFile> {
    val result = mutableListOf<RomFile>()
    val visited = mutableSetOf<Path>()
    val pending = ArrayDeque<File>()
    pending.add(root)

    while (pending.isNotEmpty()) {
        currentCoroutineContext().ensureActive()
        val directory = pending.removeFirst()
        val children = unvisitedChildren(directory, visited) ?: continue
        val scanned = scanDirectory(directory, children, allowedExtensions)
        result.addAll(scanned.roms)
        pending.addAll(scanned.subdirectories)
    }
    return result
}

/** Resolves aliases before descending so symlinks cannot create cycles or duplicate games. */
private fun unvisitedChildren(directory: File, visited: MutableSet<Path>): Array<File>? {
    val realPath =
        try {
            directory.toPath().toRealPath()
        } catch (_: IOException) {
            return null
        }
    return if (visited.add(realPath)) directory.listFiles() else null
}

private data class DirectoryScan(val roms: List<RomFile>, val subdirectories: List<File>)

private fun scanDirectory(
    directory: File,
    children: Array<File>,
    allowedExtensions: Set<String>,
): DirectoryScan {
    val roms = mutableListOf<RomFile>()
    val subdirectories = mutableListOf<File>()
    for (file in children) {
        if (file.isDirectory) {
            subdirectories.add(file)
        } else if (file.isFile) {
            val extension = file.extension.lowercase()
            if (extension in allowedExtensions) {
                if (file.nameWithoutExtension.lowercase() in GAME_SIGNATURES) {
                    // A game folder owns its contents: discard individual ROMs and stop descent.
                    return DirectoryScan(
                        listOf(RomFile(directory, directory.name, extension)),
                        emptyList(),
                    )
                }
                roms.add(RomFile(file, file.nameWithoutExtension, extension))
            }
        }
    }
    return DirectoryScan(roms, subdirectories)
}
