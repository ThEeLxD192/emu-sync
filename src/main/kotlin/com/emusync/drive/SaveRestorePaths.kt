package com.emusync.drive

import java.io.File
import java.nio.file.Files
import java.nio.file.LinkOption
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

    val requestedRoot = destinationRoot.toPath().toAbsolutePath().normalize()
    val root = if (Files.exists(requestedRoot, LinkOption.NOFOLLOW_LINKS)) {
        runCatching { requestedRoot.toRealPath() }.getOrNull() ?: return null
    } else {
        requestedRoot
    }
    val relative = segments.fold(Path.of("")) { path, segment -> path.resolve(segment) }
    val resolved = root.resolve(relative).normalize()
    if (!resolved.startsWith(root) || resolved == root) return null

    // A lexically-contained path can still leave the selected directory through a
    // symlink. Reject symlinks in every component, including the final target.
    var current = root
    for (segment in segments) {
        current = current.resolve(segment)
        if (Files.isSymbolicLink(current)) return null
    }

    // Also verify the canonical parent when it already exists. This protects against
    // non-obvious filesystem aliases and keeps accepted targets beneath the chosen root.
    val canonicalRoot = runCatching { root.toRealPath() }.getOrNull() ?: root
    val parent = resolved.parent
    if (parent != null && Files.exists(parent)) {
        val canonicalParent = runCatching { parent.toRealPath() }.getOrNull() ?: return null
        if (!canonicalParent.startsWith(canonicalRoot)) return null
    }

    return resolved.toFile()
}

/** Resolves a selected restore folder to individual local paths for the cloud files. */
fun resolveCloudRestorePaths(destinationFolder: String, cloudFileNames: List<String>): List<String> {
    if (destinationFolder.isBlank()) return emptyList()

    return cloudFileNames.mapNotNull { cloudName ->
        resolveRemoteFile(File(destinationFolder), cloudName)?.absolutePath
    }.distinct()
}
