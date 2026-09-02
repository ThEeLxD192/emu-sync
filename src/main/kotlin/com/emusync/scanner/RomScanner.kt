package com.emusync.scanner

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.coroutines.ensureActive
import java.io.File

/**
 * Data class representing a discovered ROM file.
 *
 * @property file     The ROM [File] on disk.
 * @property name     Display-friendly name (filename without extension).
 * @property extension The file extension (lowercase, without dot).
 */
data class RomFile(
    val file: File,
    val name: String,
    val extension: String,
)

/**
 * Scans a directory for ROM files matching the given extensions.
 *
 * This is a `suspend` function that offloads the I/O-heavy directory listing to
 * [Dispatchers.IO], preventing UI freezes on large ROM collections.
 *
 * @param directory  Absolute path to the directory to scan.
 * @param extensions List of allowed file extensions (case-insensitive, without dots).
 *                   Example: `listOf("gba", "gbc", "zip")`
 * @return Sorted list of [RomFile] objects found. Empty list if directory doesn't exist.
 */
suspend fun scanRoms(
    directory: String,
    extensions: List<String>,
): List<RomFile> = withContext(Dispatchers.IO) {
    val dir = File(directory)

    if (!dir.exists() || !dir.isDirectory) {
        return@withContext emptyList()
    }

    // Normalize extensions to lowercase
    val allowedExtensions = extensions.map { it.lowercase() }.toSet()

    // Signature files that indicate the current directory is a self-contained game
    val signatureFiles = setOf("ps3_disc", "eboot", "app", "main")

    val result = mutableListOf<RomFile>()
    
    // Use ArrayDeque as a queue for breadth-first search
    val directoriesToScan = ArrayDeque<File>()
    directoriesToScan.add(dir)

    while (directoriesToScan.isNotEmpty()) {
        ensureActive() 

        val currentDir = directoriesToScan.removeFirst()
        val children = currentDir.listFiles() ?: continue

        var isGameFolder = false
        val singleRomsInThisDir = mutableListOf<RomFile>()
        val subdirectories = mutableListOf<File>()

        for (file in children) {
            if (file.isDirectory) {
                subdirectories.add(file)
            } else if (file.isFile) {
                val ext = file.extension.lowercase()
                
                if (ext in allowedExtensions) {
                    val nameWithoutExt = file.nameWithoutExtension.lowercase()
                    
                    if (nameWithoutExt in signatureFiles) {
                        result.add(
                            RomFile(
                                file = currentDir, 
                                name = currentDir.name,
                                extension = ext
                            )
                        )
                        isGameFolder = true
                        break // Stop checking further files in this folder
                    } else {
                        // Classic single-file ROM
                        singleRomsInThisDir.add(
                            RomFile(
                                file = file,
                                name = file.nameWithoutExtension,
                                extension = ext
                            )
                        )
                    }
                }
            }
        }

        // If the folder was not a self-contained game (e.g., did not contain eboot.bin),
        // add its individual ROMs and continue exploring subfolders.
        if (!isGameFolder) {
            result.addAll(singleRomsInThisDir)
            directoriesToScan.addAll(subdirectories)
        }
    }

    // Return distinct list sorted alphabetically
    return@withContext result
        .distinctBy { it.file.absolutePath }
        .sortedBy { it.name.lowercase() }
}
