package com.emusync.domain.sync

private val WINDOWS_DRIVE_PATH = Regex("^[A-Za-z]:.*")

fun remotePathSegments(remoteName: String): List<String>? =
    when {
        remoteName.isBlank() || remoteName != remoteName.trim() || '\u0000' in remoteName -> null
        remoteName.startsWith('/') ||
            remoteName.startsWith('\\') ||
            WINDOWS_DRIVE_PATH.matches(remoteName) -> null
        else ->
            remoteName.split(Regex("[/\\\\]"), limit = 0).takeIf {
                it.isNotEmpty() &&
                    it.none { segment -> segment.isBlank() || segment == "." || segment == ".." }
            }
    }
