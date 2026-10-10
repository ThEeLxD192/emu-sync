package com.emusync.drive

import java.io.File
import java.nio.file.Path

private val WINDOWS_ABSOLUTE_PATH = Regex("^[A-Za-z]:[\\\\/].*")

/**
 * Resolves a Drive file name below [destinationRoot].
 *
 * Drive names are untrusted input. Both slash styles are treated as separators so a
 * name created on another operating system cannot escape the save directory.
 */
fun resolveRemoteFile(destinationRoot: File, remoteName: String): File? {
    if (remoteName.isBlank() || remoteName != remoteName.trim()) return null
    if (remoteName.startsWith('/') || remoteName.startsWith('\\') || WINDOWS_ABSOLUTE_PATH.matches(remoteName)) {
        return null
    }

    val segments = remoteName.split(Regex("[/\\\\]"), limit = 0)
    if (segments.isEmpty() || segments.any { it.isBlank() || it == "." || it == ".." }) return null

    val root = destinationRoot.toPath().toAbsolutePath().normalize()
    val relative = segments.fold(Path.of("")) { path, segment -> path.resolve(segment) }
    val resolved = root.resolve(relative).normalize()
    return resolved.takeIf { it.startsWith(root) && it != root }?.toFile()
}

/** Resolves a selected restore folder to individual local paths for the cloud files. */
fun resolveCloudRestorePaths(destinationFolder: String, cloudFileNames: List<String>): List<String> {
    if (destinationFolder.isBlank()) return emptyList()

    return cloudFileNames.mapNotNull { cloudName ->
        resolveRemoteFile(File(destinationFolder), cloudName)?.absolutePath
    }.distinct()
}
