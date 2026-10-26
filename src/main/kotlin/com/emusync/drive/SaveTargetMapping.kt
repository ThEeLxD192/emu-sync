package com.emusync.drive

import com.emusync.model.SaveTarget
import com.emusync.model.SaveTargetType
import java.io.File

/** Configuration problems must not be treated as an offline pre-sync. */
internal class SaveTargetMappingException(message: String) : IllegalArgumentException(message)

internal fun SaveTarget.isDirectory(cloudFiles: List<DriveFile>, siblings: List<SaveTarget> = emptyList()): Boolean = isDirectorySaveTarget(
    type, File(path), cloudFiles,
    reservedSiblingNames = siblings.filter { it.path != path && (it.type == SaveTargetType.FILE || File(it.path).isFile) }
        .mapTo(mutableSetOf(), ::fileCloudName),
    cloudName = cloudPath,
)

internal fun fileCloudName(target: SaveTarget): String = target.cloudPath?.takeIf { it.isNotBlank() } ?: File(target.path).name

internal fun directoryCloudName(prefix: String?, relativePath: String): String =
    prefix?.takeIf { it.isNotBlank() }?.let { "$it/$relativePath" } ?: relativePath

internal fun fileSiblingCloudNames(target: SaveTarget, siblings: List<SaveTarget>, files: List<DriveFile>): Set<String> {
    val prefix = target.cloudPath?.takeIf { target.isDirectory(files, siblings) && it.isNotBlank() }
    return siblings.filter { it.path != target.path && !it.isDirectory(files, siblings) }.mapNotNull { other ->
        val name = fileCloudName(other)
        when {
            prefix == null -> name
            name.startsWith("$prefix/") -> name.removePrefix("$prefix/")
            else -> null
        }
    }.toSet()
}

/** Projects remote keys into paths relative to one directory without changing file identities. */
internal fun projectDirectoryFiles(files: List<DriveFile>, prefix: String?): List<DriveFile> {
    if (prefix.isNullOrBlank()) return files
    return files.filter { it.name.startsWith("$prefix/") }.map { it.copy(name = it.name.removePrefix("$prefix/")) }
}

/** Sibling exclusions use remote keys; local directory names need not match across devices. */
internal fun reservedCloudNames(target: SaveTarget, siblings: List<SaveTarget>, cloudFiles: List<DriveFile>): Set<String> {
    val prefix = target.cloudPath?.takeIf { target.isDirectory(cloudFiles, siblings) && it.isNotBlank() }
    fun project(name: String): String? = when {
        prefix == null -> name
        name.startsWith("$prefix/") -> name.removePrefix("$prefix/")
        else -> null
    }
    val directories = siblings.filter { it.isDirectory(cloudFiles, siblings) && it.cloudPath == target.cloudPath }
    val reserved = getSiblingReservedNames(File(target.path), directories.map { File(it.path) to it.type }).toMutableSet()
    for (other in siblings) {
        if (other.path == target.path && other.cloudPath == target.cloudPath) continue
        if (!other.isDirectory(cloudFiles, siblings)) {
            project(fileCloudName(other))?.let { reserved += it }
        } else if (!other.cloudPath.isNullOrBlank() && other.cloudPath != target.cloudPath) {
            cloudFiles.filter { it.name.startsWith("${other.cloudPath}/") }.forEach { file ->
                project(file.name)?.let { reserved += it }
            }
        }
    }
    return reserved
}

/** Rejects legacy layouts whose directory ownership cannot be recovered without guessing. */
internal fun validateSaveTargetMappings(targets: List<SaveTarget>, cloudFiles: List<DriveFile>) {
    for (target in targets) {
        target.cloudPath?.takeIf { it.isNotBlank() }?.let { path ->
            if (remotePathSegments(path) == null) {
                throw SaveTargetMappingException("Invalid cloud path '$path'. Use a relative path without '..' or absolute components.")
            }
        }
    }
    fun localPath(target: SaveTarget) = File(target.path).toPath().toAbsolutePath().normalize()
    val localCollisions = targets.groupBy(::localPath).values.any { aliases ->
        aliases.map {
            if (it.isDirectory(cloudFiles, targets)) "directory:${it.cloudPath.orEmpty()}" else "file:${fileCloudName(it)}"
        }.distinct().size > 1
    }
    if (localCollisions) throw SaveTargetMappingException("Different cloud paths cannot restore to the same local save path. Choose separate destinations.")
    for (file in targets.filterNot { it.isDirectory(cloudFiles, targets) }) {
        if (targets.any { localPath(it) != localPath(file) && localPath(it).startsWith(localPath(file)) }) {
            throw SaveTargetMappingException("A save file cannot also be a parent directory for another save. Choose separate destinations.")
        }
    }
    val directories = targets.filter { it.isDirectory(cloudFiles, targets) }.distinctBy { File(it.path).absoluteFile.normalize().path to it.cloudPath }
    val scoped = directories.filter { !it.cloudPath.isNullOrBlank() }
    for ((index, target) in scoped.withIndex()) {
        if (scoped.drop(index + 1).any {
            it.cloudPath == target.cloudPath || it.cloudPath!!.startsWith("${target.cloudPath}/") || target.cloudPath!!.startsWith("${it.cloudPath}/")
        }) throw SaveTargetMappingException("Each save directory needs a separate cloud path. Overlapping paths would mix their saves.")
    }
    for (target in directories) {
        val reserved = reservedCloudNames(target, targets, cloudFiles)
        val paths = projectDirectoryFiles(cloudFiles, target.cloudPath).filterNot { it.name in reserved }.mapNotNull { file ->
            resolveRemoteFile(File(target.path), file.name)?.toPath()?.normalize()?.let { it to file.name }
        }
        val aliases = paths.groupBy { it.first }.values.any { names -> names.map { it.second }.distinct().size > 1 }
        val destinations = paths.mapTo(mutableSetOf()) { it.first }
        val nestedFiles = destinations.any { path -> generateSequence(path.parent) { it.parent }.any { it in destinations } }
        if (aliases || nestedFiles) throw SaveTargetMappingException("Cloud files resolve to conflicting local paths. Choose separate destinations before restoring them.")
    }
    val legacy = directories.filter { it.cloudPath.isNullOrBlank() }
    if (legacy.size < 2) return
    val fileNames = targets.filterNot { it.isDirectory(cloudFiles, targets) }.map(::fileCloudName).toSet()
    val legacyFiles = cloudFiles.filterNot { file ->
        file.name in fileNames || scoped.any { file.name.startsWith("${it.cloudPath}/") }
    }
    val missingRoot = legacy.any { !File(it.path).isDirectory }
    val ambiguousLocal = legacy.flatMap { target ->
        val root = File(target.path)
        if (!root.isDirectory) emptyList() else root.walkTopDown().filter { it.isFile }
            .map { it.toRelativeString(root).replace(File.separatorChar, '/') }.toList()
    }.groupingBy { it }.eachCount().any { it.value > 1 }
    val unassignedRemote = legacyFiles.any { remote -> legacy.none { resolveRemoteFile(File(it.path), remote.name)?.isFile == true } }
    if ((missingRoot && legacyFiles.isNotEmpty()) || ambiguousLocal || unassignedRemote) {
        throw SaveTargetMappingException(
            "These legacy save directories share a cloud location, so their files cannot be assigned safely. " +
                "Restore the cloud files individually, or reconnect the original local directories before assigning separate cloud paths. " +
                "These directories were not synchronized.",
        )
    }
}
