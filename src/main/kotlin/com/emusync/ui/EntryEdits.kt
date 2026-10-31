package com.emusync.ui

import com.emusync.model.AppConfig
import com.emusync.model.EmulatorSystem
import com.emusync.model.GameEntry
import com.emusync.model.NativePCGame
import com.emusync.model.effectiveCloudFolder

/** Regrouping a shared root is a display edit; preserve its existing cloud location. */
internal fun preserveSharedCloudRoot(
    config: AppConfig,
    oldEntry: GameEntry,
    newEntry: GameEntry,
): GameEntry {
    val sourceUsesRoot =
        oldEntry is EmulatorSystem ||
            (oldEntry is NativePCGame && (oldEntry.group == null || oldEntry.cloudFolder != null))
    if (!sourceUsesRoot || !newEntry.cloudFolder.isNullOrBlank()) return newEntry
    val otherEntries = config.entries.filterNot { it === oldEntry || it.name == oldEntry.name }
    val sharedRootChanged =
        oldEntry.effectiveCloudFolder != newEntry.effectiveCloudFolder &&
            otherEntries.any { it.effectiveCloudFolder == oldEntry.effectiveCloudFolder }
    return if (sharedRootChanged) {
        when (newEntry) {
            is EmulatorSystem -> newEntry.copy(cloudFolder = oldEntry.effectiveCloudFolder)
            is NativePCGame -> newEntry.copy(cloudFolder = oldEntry.effectiveCloudFolder)
        }
    } else newEntry
}

/** Builds only the requested overrides; null means keep an override and blank means clear it. */
internal fun buildGameEntry(game: GameItem, changes: GameOverrideChanges): GameEntry? =
    when (val entry = game.entry) {
        is EmulatorSystem -> game.romFile?.name?.let { updateEmulatorOverrides(entry, it, changes) }
        is NativePCGame ->
            entry.copy(
                name = changes.title?.trim()?.takeIf { it.isNotBlank() } ?: entry.name,
                savePaths = changes.targets.map { it.path },
                saveTargets = changes.targets,
                coverPath =
                    if (changes.coverPath != null) changes.coverPath.trim().ifBlank { null }
                    else entry.coverPath,
            )
    }

private fun updateEmulatorOverrides(
    entry: EmulatorSystem,
    romName: String,
    changes: GameOverrideChanges,
): EmulatorSystem {
    val savePaths = changes.targets.map { it.path.trim() }.filter { it.isNotBlank() }
    val pathsByRom = entry.savePathsByRom.toMutableMap()
    val targetsByRom = entry.saveTargetsByRom.toMutableMap()
    if (savePaths.isEmpty()) {
        pathsByRom.remove(romName)
        targetsByRom.remove(romName)
    } else {
        pathsByRom[romName] = savePaths
        targetsByRom[romName] = changes.targets.filter { it.path.isNotBlank() }
    }
    return entry.copy(
        savePathsByRom = pathsByRom,
        saveTargetsByRom = targetsByRom,
        coverPathByRom = entry.coverPathByRom.withOptionalOverride(romName, changes.coverPath),
        titleByRom = entry.titleByRom.withOptionalOverride(romName, changes.title),
    )
}

private fun Map<String, String>.withOptionalOverride(
    key: String,
    value: String?,
): Map<String, String> {
    val updated = toMutableMap()
    if (value != null) {
        val clean = value.trim().takeIf { it.isNotBlank() }
        if (clean != null) updated[key] = clean else updated.remove(key)
    }
    return updated
}
