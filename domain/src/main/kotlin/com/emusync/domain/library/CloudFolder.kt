package com.emusync.domain.library

/**
 * Returns the effective Google Drive folder name used for storing saves. Priority:
 * [GameEntry.cloudFolder] > [GameEntry.group] > [GameEntry.name].
 */
val GameEntry.effectiveCloudFolder: String
    get() =
        cloudFolder?.trim()?.takeIf { it.isNotBlank() }
            ?: group?.trim()?.takeIf { it.isNotBlank() }
            ?: name
