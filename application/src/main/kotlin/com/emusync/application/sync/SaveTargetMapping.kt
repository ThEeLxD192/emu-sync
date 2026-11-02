package com.emusync.application.sync

import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetType
import com.emusync.domain.sync.CloudFile
import com.emusync.domain.sync.LocalSaveAccess
import com.emusync.domain.sync.LocalSavePath
import com.emusync.domain.sync.remotePathSegments

internal class SaveTargetMapping(private val files: LocalSaveAccess) {
    fun getSiblingReservedNames(
        targetPath: LocalSavePath,
        allTargets: Iterable<Pair<LocalSavePath, SaveTargetType>>,
    ): Set<String> {
        val reserved = mutableSetOf<String>()
        val targetNormalized = targetPath.normalizedPath

        for ((otherFile, otherType) in allTargets) {
            if (otherFile.normalizedPath == targetNormalized) {
                continue
            }
            val isOtherDir =
                if (otherFile.exists()) {
                    otherFile.isDirectory
                } else {
                    otherType == SaveTargetType.DIRECTORY
                }

            if (isOtherDir) {
                reserved += newerSiblingPaths(otherFile, targetPath)
            } else {
                reserved.add(otherFile.name)
            }
        }
        return reserved
    }

    private fun newerSiblingPaths(directory: LocalSavePath, target: LocalSavePath): Set<String> {
        if (!directory.isDirectory) return emptySet()
        return directory
            .walkTopDown()
            .filter { it.isFile }
            .mapNotNull { file ->
                val relative = file.toRelativeString(directory)
                val destination = target.resolve(relative)
                relative.takeIf {
                    !destination.exists() || file.lastModified() > destination.lastModified()
                }
            }
            .toSet()
    }

    fun isDirectorySaveTarget(
        type: SaveTargetType,
        path: LocalSavePath,
        cloudFiles: List<CloudFile>,
        reservedSiblingNames: Set<String> = emptySet(),
        cloudName: String? = null,
    ): Boolean {
        if (path.exists()) {
            return path.isDirectory
        }
        return when (type) {
            SaveTargetType.DIRECTORY -> true
            SaveTargetType.FILE -> false
            SaveTargetType.UNKNOWN ->
                cloudFiles.none { cloudFile ->
                    cloudFile.name == (cloudName ?: path.name) &&
                        cloudFile.name !in reservedSiblingNames
                }
        }
    }

    fun cloudFilesForSaveTarget(
        type: SaveTargetType,
        path: LocalSavePath,
        cloudFiles: List<CloudFile>,
        reservedSiblingNames: Set<String> = emptySet(),
    ): List<CloudFile> {
        if (!isDirectorySaveTarget(type, path, cloudFiles, reservedSiblingNames)) return cloudFiles
        return cloudFiles.filterNot { it.name in reservedSiblingNames }
    }

    fun isDirectory(
        target: SaveTarget,
        cloudFiles: List<CloudFile>,
        siblings: List<SaveTarget> = emptyList(),
    ): Boolean =
        isDirectorySaveTarget(
            target.type,
            files.path(target.path),
            cloudFiles,
            reservedSiblingNames =
                siblings
                    .filter {
                        it.path != target.path &&
                            (it.type == SaveTargetType.FILE || files.path(it.path).isFile)
                    }
                    .mapTo(mutableSetOf(), { fileCloudName(it) }),
            cloudName = target.cloudPath,
        )

    fun fileCloudName(target: SaveTarget): String =
        target.cloudPath?.takeIf { it.isNotBlank() } ?: files.path(target.path).name

    fun fileSiblingCloudNames(
        target: SaveTarget,
        siblings: List<SaveTarget>,
        files: List<CloudFile>,
    ): Set<String> {
        val prefix =
            target.cloudPath?.takeIf { isDirectory(target, files, siblings) && it.isNotBlank() }
        return siblings
            .filter { it.path != target.path && !isDirectory(it, files, siblings) }
            .mapNotNull { other ->
                val name = fileCloudName(other)
                when {
                    prefix == null -> name
                    name.startsWith("$prefix/") -> name.removePrefix("$prefix/")
                    else -> null
                }
            }
            .toSet()
    }

    /** Sibling exclusions use remote keys; local directory names need not match across devices. */
    fun reservedCloudNames(
        target: SaveTarget,
        siblings: List<SaveTarget>,
        cloudFiles: List<CloudFile>,
    ): Set<String> {
        val prefix =
            target.cloudPath?.takeIf {
                isDirectory(target, cloudFiles, siblings) && it.isNotBlank()
            }
        val directories = siblings.filter {
            isDirectory(it, cloudFiles, siblings) && it.cloudPath == target.cloudPath
        }
        val reserved =
            getSiblingReservedNames(
                    files.path(target.path),
                    directories.map { files.path(it.path) to it.type },
                )
                .toMutableSet()
        for (other in siblings) {
            if (other.path == target.path && other.cloudPath == target.cloudPath) continue
            if (!isDirectory(other, cloudFiles, siblings)) {
                projectCloudName(fileCloudName(other), prefix)?.let { reserved += it }
            } else if (!other.cloudPath.isNullOrBlank() && other.cloudPath != target.cloudPath) {
                cloudFiles
                    .filter { it.name.startsWith("${other.cloudPath}/") }
                    .forEach { file ->
                        projectCloudName(file.name, prefix)?.let { reserved += it }
                    }
            }
        }
        return reserved
    }

    private fun projectCloudName(name: String, prefix: String?): String? =
        when {
            prefix == null -> name
            name.startsWith("$prefix/") -> name.removePrefix("$prefix/")
            else -> null
        }

    /** Rejects legacy layouts whose directory ownership cannot be recovered without guessing. */
    fun validateSaveTargetMappings(targets: List<SaveTarget>, cloudFiles: List<CloudFile>) {
        SaveTargetValidator(targets, cloudFiles).validate()
    }

    private inner class SaveTargetValidator(
        private val targets: List<SaveTarget>,
        private val cloudFiles: List<CloudFile>,
    ) {
        fun validate() {
            validateCloudPaths()
            validateLocalDestinations()
            val directories =
                targets
                    .filter { isDirectory(it, cloudFiles, targets) }
                    .distinctBy {
                        files.path(it.path).normalizedPath to it.cloudPath
                    }
            val scoped = directories.filter { !it.cloudPath.isNullOrBlank() }
            validateDirectoryScopes(scoped)
            directories.forEach(::validateDirectoryFiles)
            validateLegacyDirectories(directories, scoped)
        }

        private fun localPath(target: SaveTarget) = files.path(target.path).normalizedPath

        private fun validateCloudPaths() {
            for (target in targets) {
                target.cloudPath
                    ?.takeIf { it.isNotBlank() }
                    ?.let { path ->
                        if (remotePathSegments(path) == null) {
                            throw SaveTargetMappingException(
                                "Invalid cloud path '$path'. Use a relative path without '..' or absolute components."
                            )
                        }
                    }
            }
        }

        private fun validateLocalDestinations() {
            val localCollisions =
                targets.groupBy(::localPath).values.any { aliases ->
                    aliases
                        .map {
                            if (isDirectory(it, cloudFiles, targets))
                                "directory:${it.cloudPath.orEmpty()}"
                            else "file:${fileCloudName(it)}"
                        }
                        .distinct()
                        .size > 1
                }
            if (localCollisions)
                throw SaveTargetMappingException(
                    "Different cloud paths cannot restore to the same local save path. Choose separate destinations."
                )
            for (file in targets.filterNot { isDirectory(it, cloudFiles, targets) }) {
                if (
                    targets.any {
                        localPath(it) != localPath(file) &&
                            localPath(file) in files.path(it.path).ancestorPaths
                    }
                ) {
                    throw SaveTargetMappingException(
                        "A save file cannot also be a parent directory for another save. Choose separate destinations."
                    )
                }
            }
        }

        private fun validateDirectoryScopes(scoped: List<SaveTarget>) {
            for ((index, target) in scoped.withIndex()) {
                val targetPath = checkNotNull(target.cloudPath)
                if (
                    scoped.drop(index + 1).any {
                        val otherPath = checkNotNull(it.cloudPath)
                        otherPath == targetPath ||
                            otherPath.startsWith("$targetPath/") ||
                            targetPath.startsWith("$otherPath/")
                    }
                )
                    throw SaveTargetMappingException(
                        "Each save directory needs a separate cloud path. Overlapping paths would mix their saves."
                    )
            }
        }

        private fun validateDirectoryFiles(target: SaveTarget) {
            val reserved = reservedCloudNames(target, targets, cloudFiles)
            val paths =
                projectDirectoryFiles(cloudFiles, target.cloudPath)
                    .filterNot { it.name in reserved }
                    .mapNotNull { file ->
                        files.path(target.path).resolveRemote(file.name)?.normalizedPath?.let {
                            it to file.name
                        }
                    }
            val aliases =
                paths
                    .groupBy { it.first }
                    .values
                    .any { names -> names.map { it.second }.distinct().size > 1 }
            val destinations = paths.mapTo(mutableSetOf()) { it.first }
            val nestedFiles = destinations.any { path ->
                files.path(path).ancestorPaths.any { it in destinations }
            }
            if (aliases || nestedFiles)
                throw SaveTargetMappingException(
                    "Cloud files resolve to conflicting local paths. Choose separate destinations before restoring them."
                )
        }

        private fun validateLegacyDirectories(
            directories: List<SaveTarget>,
            scoped: List<SaveTarget>,
        ) {
            val legacy = directories.filter { it.cloudPath.isNullOrBlank() }
            if (legacy.size < 2) return
            val fileNames =
                targets
                    .filterNot { isDirectory(it, cloudFiles, targets) }
                    .map({ fileCloudName(it) })
                    .toSet()
            val legacyFiles = cloudFiles.filterNot { file ->
                file.name in fileNames || scoped.any { file.name.startsWith("${it.cloudPath}/") }
            }
            val missingRoot = legacy.any { !files.path(it.path).isDirectory }
            val ambiguousLocal =
                legacy
                    .flatMap { target ->
                        val root = files.path(target.path)
                        if (!root.isDirectory) emptyList()
                        else
                            root
                                .walkTopDown()
                                .filter { it.isFile }
                                .map { it.toRelativeString(root) }
                                .toList()
                    }
                    .groupingBy { it }
                    .eachCount()
                    .any { it.value > 1 }
            val unassignedRemote = legacyFiles.any { remote ->
                legacy.none { files.path(it.path).resolveRemote(remote.name)?.isFile == true }
            }
            val missingRemoteFiles = missingRoot && legacyFiles.isNotEmpty()
            if (missingRemoteFiles || ambiguousLocal || unassignedRemote) {
                throw SaveTargetMappingException(
                    "These legacy save directories share a cloud location, so their files cannot be assigned safely. " +
                        "Restore the cloud files individually, or reconnect the original local directories before assigning separate cloud paths. " +
                        "These directories were not synchronized."
                )
            }
        }
    }
}
