package com.emusync.local.saves

import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetType
import java.io.File

/** Used only while migrating legacy string-only save paths. */
fun inferLegacySaveTarget(path: String): SaveTarget {
    val file = File(path)
    val type =
        when {
            file.exists() -> if (file.isDirectory) SaveTargetType.DIRECTORY else SaveTargetType.FILE
            else -> SaveTargetType.UNKNOWN
        }
    return SaveTarget(path, type)
}

private fun SaveTarget.resolveUnknownTypeIfPresent(): SaveTarget {
    val file = File(path)
    val normalized = if (cloudPath?.isBlank() == true) copy(cloudPath = null) else this
    if (!file.exists()) return normalized
    val actualType = if (file.isDirectory) SaveTargetType.DIRECTORY else SaveTargetType.FILE
    val needsTypeUpdate =
        when (type) {
            SaveTargetType.UNKNOWN -> true
            SaveTargetType.FILE -> file.isDirectory
            SaveTargetType.DIRECTORY -> file.isFile
        }
    return if (needsTypeUpdate) {
        normalized.copy(type = actualType)
    } else {
        normalized
    }
}

fun GameEntry.effectiveSaveTargets(): List<SaveTarget> =
    (saveTargets.ifEmpty { savePaths.filter { it.isNotBlank() }.map(::inferLegacySaveTarget) }).map(
        SaveTarget::resolveUnknownTypeIfPresent
    )

fun EmulatorSystem.effectiveSaveTargetsForRom(romName: String): List<SaveTarget> =
    (saveTargetsByRom[romName]?.map(SaveTarget::resolveUnknownTypeIfPresent)
            ?: saveTargetsByRom[romName.substringBeforeLast('.', romName)]?.map(
                SaveTarget::resolveUnknownTypeIfPresent
            )
            ?: savePathsByRom[romName]?.filter { it.isNotBlank() }?.map(::inferLegacySaveTarget)
            ?: savePathsByRom[romName.substringBeforeLast('.', romName)]
                ?.filter { it.isNotBlank() }
                ?.map(::inferLegacySaveTarget))
        .orEmpty()

/** Adds explicit types to legacy paths without changing their path values. */
fun GameEntry.migrateLegacySaveTargets(): GameEntry =
    when (this) {
        is EmulatorSystem ->
            copy(
                saveTargets =
                    (saveTargets.ifEmpty {
                            savePaths.filter { it.isNotBlank() }.map(::inferLegacySaveTarget)
                        })
                        .map(SaveTarget::resolveUnknownTypeIfPresent),
                saveTargetsByRom =
                    saveTargetsByRom.toMutableMap().apply {
                        replaceAll { _, targets ->
                            targets.map(SaveTarget::resolveUnknownTypeIfPresent)
                        }
                        savePathsByRom.forEach { (rom, paths) ->
                            if (rom !in this && paths.isNotEmpty()) {
                                this[rom] =
                                    paths.filter { it.isNotBlank() }.map(::inferLegacySaveTarget)
                            }
                        }
                    },
            )
        is NativePCGame ->
            copy(
                saveTargets =
                    (saveTargets.ifEmpty {
                            savePaths.filter { it.isNotBlank() }.map(::inferLegacySaveTarget)
                        })
                        .map(SaveTarget::resolveUnknownTypeIfPresent)
            )
    }
