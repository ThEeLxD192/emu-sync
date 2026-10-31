package com.emusync.application.sync

import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.effectiveCloudFolder
import com.emusync.domain.sync.LocalSaveAccess
import com.emusync.domain.sync.SyncGame
import com.emusync.domain.sync.cloudPathSegments
import com.emusync.domain.sync.sanitizeCloudFolderName

internal class SaveTargetResolver(private val files: LocalSaveAccess) {
    fun getCloudPathSegments(
        entry: GameEntry,
        romName: String? = null,
        gameTitle: String? = null,
    ): List<String> =
        cloudPathSegments(
            entry,
            romName,
            gameTitle,
            files.effectiveSaveTargets(entry),
            (entry as? EmulatorSystem)
                ?.let { system -> romName?.let { files.effectiveSaveTargetsForRom(system, it) } }
                .orEmpty(),
        )

    fun getCloudPathSegments(item: SyncGame): List<String> {
        val romName = item.romPath?.let { files.path(it).name } ?: item.name
        return getCloudPathSegments(item.entry, romName, item.effectiveTitle)
    }

    fun getSaveTargets(entry: GameEntry): List<ResolvedSaveTarget> {
        return when (entry) {
            is NativePCGame -> {
                val cloudPath = getCloudPathSegments(entry)
                files.effectiveSaveTargets(entry).map {
                    ResolvedSaveTarget(
                        files.path(it.path),
                        it.type,
                        cloudPath,
                        entry.name,
                        it.cloudPath,
                    )
                }
            }
            is EmulatorSystem -> {
                val targets = mutableListOf<ResolvedSaveTarget>()

                // 1. Global emulator save paths (if any)
                if (files.effectiveSaveTargets(entry).isNotEmpty()) {
                    val globalPath = listOf(entry.effectiveCloudFolder)
                    for (target in files.effectiveSaveTargets(entry)) {
                        targets.add(
                            ResolvedSaveTarget(
                                files.path(target.path),
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
                    for (target in files.effectiveSaveTargetsForRom(entry, romName)) {
                        targets.add(
                            ResolvedSaveTarget(
                                files.path(target.path),
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
