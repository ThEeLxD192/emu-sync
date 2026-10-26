package com.emusync.drive

import com.emusync.model.SaveTarget
import com.emusync.model.SaveTargetType
import java.io.File
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.Path

private val WINDOWS_DRIVE_PATH = Regex("^[A-Za-z]:.*")

internal fun remotePathSegments(remoteName: String): List<String>? {
    if (remoteName.isBlank() || remoteName != remoteName.trim() || '\u0000' in remoteName) return null
    if (remoteName.startsWith('/') || remoteName.startsWith('\\') || WINDOWS_DRIVE_PATH.matches(remoteName)) return null
    val segments = remoteName.split(Regex("[/\\\\]"), limit = 0)
    return segments.takeIf { it.isNotEmpty() && it.none { segment -> segment.isBlank() || segment == "." || segment == ".." } }
}

/**
 * Resolves a Drive file name below [destinationRoot].
 *
 * Drive names are untrusted input. Both slash styles are treated as separators so a
 * name created on another operating system cannot escape the save directory.
 */
fun resolveRemoteFile(destinationRoot: File, remoteName: String): File? {
    val segments = remotePathSegments(remoteName) ?: return null

    // Preserve the selected path's spelling. On Windows, toRealPath() can expand an
    // 8.3 path (for example RUNNER~1) and break callers' path comparisons.
    val root = destinationRoot.toPath().toAbsolutePath().normalize()
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
    val canonicalRoot = if (Files.exists(root, LinkOption.NOFOLLOW_LINKS)) {
        runCatching { root.toRealPath() }.getOrNull() ?: return null
    } else {
        root
    }
    val parent = resolved.parent
    if (parent != null && Files.exists(parent)) {
        val canonicalParent = runCatching { parent.toRealPath() }.getOrNull() ?: return null
        if (!canonicalParent.startsWith(canonicalRoot)) return null
    }

    return resolved.toFile()
}

/** Resolves a selected restore folder to individual local paths for the cloud files. */
fun resolveCloudRestorePaths(destinationFolder: String, cloudFileNames: List<String>): List<String> {
    return resolveCloudRestoreTargets(destinationFolder, cloudFileNames).map { it.path }.distinct()
}

/** Keeps each exact Drive name, including its nested path, when restoring individual files. */
fun resolveCloudRestoreTargets(destinationFolder: String, cloudFileNames: List<String>): List<SaveTarget> {
    if (destinationFolder.isBlank()) return emptyList()

    return cloudFileNames.mapNotNull { cloudName ->
        resolveRemoteFile(File(destinationFolder), cloudName)?.let { local ->
            SaveTarget(local.absolutePath, SaveTargetType.FILE, cloudPath = cloudName)
        }
    }.distinct()
}
