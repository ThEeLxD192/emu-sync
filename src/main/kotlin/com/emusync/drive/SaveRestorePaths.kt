package com.emusync.drive

import java.io.File

/** Resolves a selected restore folder to individual local paths for the cloud files. */
fun resolveCloudRestorePaths(destinationFolder: String, cloudFileNames: List<String>): List<String> {
    if (destinationFolder.isBlank()) return emptyList()

    val root = File(destinationFolder).toPath().toAbsolutePath().normalize()
    return cloudFileNames.mapNotNull { cloudName ->
        if (cloudName.isBlank()) return@mapNotNull null

        val relativeName = cloudName.replace('/', File.separatorChar).replace('\\', File.separatorChar)
        val resolved = root.resolve(relativeName).normalize()
        resolved.takeIf { it.startsWith(root) && it != root }?.toString()
    }.distinct()
}
