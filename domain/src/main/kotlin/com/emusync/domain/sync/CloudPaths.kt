package com.emusync.domain.sync

import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.effectiveCloudFolder

fun sanitizeCloudFolderName(name: String): String {
    return name.replace('/', '-').replace('\\', '-').trim().ifBlank { "Unknown" }
}

fun cloudPathSegments(
    entry: GameEntry,
    romName: String? = null,
    gameTitle: String? = null,
    globalTargets: List<SaveTarget>,
    romTargets: List<SaveTarget>,
): List<String> {
    val baseFolder = entry.effectiveCloudFolder
    return when (entry) {
        is EmulatorSystem -> {
            val globalPaths =
                globalTargets.map { normalizeSavePath(it.path) to it.cloudPath }.toSet()
            val romPaths = romTargets
            val hasDistinctRomPath = romPaths.any {
                (normalizeSavePath(it.path) to it.cloudPath) !in globalPaths
            }

            // A configured global save path uses the emulator folder unless this ROM has
            // a real, distinct path of its own. Empty or copied global-path entries are
            // legacy per-ROM override records, not per-game save configuration.
            if (romName != null && (hasDistinctRomPath || globalPaths.isEmpty())) {
                val title = gameTitle ?: entry.getEffectiveTitle(romName, romName)
                listOf(baseFolder, sanitizeCloudFolderName(title))
            } else {
                listOf(baseFolder)
            }
        }
        is NativePCGame -> {
            if (entry.group != null && entry.cloudFolder == null) {
                listOf(baseFolder, sanitizeCloudFolderName(entry.name))
            } else {
                listOf(baseFolder)
            }
        }
    }
}

private fun normalizeSavePath(path: String): String =
    path.trim().replace('\\', '/').trimEnd('/').ifBlank { "/" }
