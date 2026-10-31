package com.emusync.drive

import com.emusync.drive.SyncOrchestrator.SaveTarget
import com.emusync.model.EmulatorSystem
import com.emusync.model.GameEntry
import com.emusync.model.NativePCGame
import com.emusync.model.effectiveCloudFolder
import com.emusync.model.effectiveSaveTargets
import com.emusync.model.effectiveSaveTargetsForRom
import com.emusync.ui.GameItem
import java.io.File

internal class SaveTargetResolver {
    fun sanitizeCloudFolderName(name: String): String {
        return name.replace('/', '-').replace('\\', '-').trim().ifBlank { "Unknown" }
    }

    fun getCloudPathSegments(
        entry: GameEntry,
        romName: String? = null,
        gameTitle: String? = null,
    ): List<String> {
        val baseFolder = entry.effectiveCloudFolder
        return when (entry) {
            is EmulatorSystem -> {
                val globalPaths =
                    entry
                        .effectiveSaveTargets()
                        .map { normalizeSavePath(it.path) to it.cloudPath }
                        .toSet()
                val romPaths =
                    romName
                        ?.let { name ->
                            entry.effectiveSaveTargetsForRom(name)
                        }
                        .orEmpty()
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

    fun getCloudPathSegments(item: GameItem): List<String> {
        val romName = item.romFile?.name ?: item.name
        return getCloudPathSegments(item.entry, romName, item.effectiveTitle)
    }

    fun getSaveTargets(entry: GameEntry): List<SaveTarget> {
        return when (entry) {
            is NativePCGame -> {
                val cloudPath = getCloudPathSegments(entry)
                entry.effectiveSaveTargets().map {
                    SaveTarget(File(it.path), it.type, cloudPath, entry.name, it.cloudPath)
                }
            }
            is EmulatorSystem -> {
                val targets = mutableListOf<SaveTarget>()

                // 1. Global emulator save paths (if any)
                if (entry.effectiveSaveTargets().isNotEmpty()) {
                    val globalPath = listOf(entry.effectiveCloudFolder)
                    for (target in entry.effectiveSaveTargets()) {
                        targets.add(
                            SaveTarget(
                                File(target.path),
                                target.type,
                                globalPath,
                                entry.name,
                                target.cloudPath,
                            )
                        )
                    }
                }

                // 2. Per-ROM save paths
                val romNames = entry.saveTargetsByRom.keys + entry.savePathsByRom.keys
                for (romName in romNames) {
                    val title = entry.getEffectiveTitle(romName, romName)
                    val romCloudPath =
                        listOf(entry.effectiveCloudFolder, sanitizeCloudFolderName(title))
                    for (target in entry.effectiveSaveTargetsForRom(romName)) {
                        targets.add(
                            SaveTarget(
                                File(target.path),
                                target.type,
                                romCloudPath,
                                title,
                                target.cloudPath,
                            )
                        )
                    }
                }

                targets.distinctBy { it.file.absolutePath to it.cloudPath }
            }
        }
    }

    /** Helper to retrieve all configured save paths for an entry. */
    fun getAllSavePaths(entry: GameEntry): List<File> {
        return getSaveTargets(entry).map { it.file }
    }
}
