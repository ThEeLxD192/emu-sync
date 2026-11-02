package com.emusync.application.sync

import com.emusync.domain.sync.CloudFile

internal fun directoryCloudName(prefix: String?, relativePath: String): String =
    prefix?.takeIf { it.isNotBlank() }?.let { "$it/$relativePath" } ?: relativePath

/** Projects remote keys into paths relative to one directory without changing file identities. */
internal fun projectDirectoryFiles(files: List<CloudFile>, prefix: String?): List<CloudFile> {
    if (prefix.isNullOrBlank()) return files
    return files
        .filter { it.name.startsWith("$prefix/") }
        .map { it.copy(name = it.name.removePrefix("$prefix/")) }
}
