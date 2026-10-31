package com.emusync.drive

import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.effectiveCloudFolder
import com.emusync.domain.sync.SyncGame
import com.emusync.domain.sync.cloudPathSegments
import com.emusync.domain.sync.sanitizeCloudFolderName
import com.emusync.model.effectiveSaveTargets
import com.emusync.model.effectiveSaveTargetsForRom
import java.io.File

internal class SaveTargetResolver {
    fun getCloudPathSegments(
        entry: GameEntry,
        romName: String? = null,
        gameTitle: String? = null,
    ): List<String> =
        cloudPathSegments(
            entry,
            romName,
            gameTitle,
            entry.effectiveSaveTargets(),
            (entry as? EmulatorSystem)
                ?.let { system -> romName?.let { system.effectiveSaveTargetsForRom(it) } }
                .orEmpty(),
        )

    fun getCloudPathSegments(item: SyncGame): List<String> {
        val romName = item.romPath?.let { File(it).name } ?: item.name
        return getCloudPathSegments(item.entry, romName, item.effectiveTitle)
    }

    fun getSaveTargets(entry: GameEntry): List<ResolvedSaveTarget> {
        return when (entry) {
            is NativePCGame -> {
                val cloudPath = getCloudPathSegments(entry)
                entry.effectiveSaveTargets().map {
                    ResolvedSaveTarget(File(it.path), it.type, cloudPath, entry.name, it.cloudPath)
                }
            }
            is EmulatorSystem -> {
                val targets = mutableListOf<ResolvedSaveTarget>()

                // 1. Global emulator save paths (if any)
                if (entry.effectiveSaveTargets().isNotEmpty()) {
                    val globalPath = listOf(entry.effectiveCloudFolder)
                    for (target in entry.effectiveSaveTargets()) {
                        targets.add(
                            ResolvedSaveTarget(
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
                            ResolvedSaveTarget(
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
}
