package com.emusync.ui.library

import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetAccess
import java.io.File

/**
 * Represents a displayable game item in the grid. Unifies ROMs (from scanning) and native games
 * into a single UI model.
 */
data class GameItem(
    val name: String,
    val entry: GameEntry,
    val romFile: File? = null,
    val saveTargets: SaveTargetAccess,
) {
    // Borrowed capabilities do not change the identity of a displayed game.
    override fun equals(other: Any?): Boolean =
        other is GameItem && name == other.name && entry == other.entry && romFile == other.romFile

    override fun hashCode(): Int {
        var result = name.hashCode()
        result = 31 * result + entry.hashCode()
        return 31 * result + (romFile?.hashCode() ?: 0)
    }

    val effectiveTitle: String
        get() =
            when (entry) {
                is EmulatorSystem -> {
                    val romName = romFile?.name ?: name
                    entry.getEffectiveTitle(romName, name)
                }
                is NativePCGame -> entry.name
            }

    val effectiveSavePaths: List<String>
        get() = effectiveSaveTargets.map { it.path }

    val effectiveSaveTargets: List<SaveTarget>
        get() =
            when (entry) {
                is NativePCGame -> saveTargets.forEntry(entry)
                is EmulatorSystem -> {
                    val romName = romFile?.name
                    val romTargets =
                        if (romName != null) saveTargets.forRom(entry, romName) else emptyList()

                    if (romTargets.isNotEmpty()) {
                        romTargets
                    } else if (saveTargets.forEntry(entry).isNotEmpty()) {
                        saveTargets.forEntry(entry)
                    } else {
                        emptyList()
                    }
                }
            }

    /** Save paths configured specifically for this ROM, without emulator-wide fallback paths. */
    val gameSpecificSavePaths: List<String>
        get() = gameSpecificSaveTargets.map { it.path }

    val gameSpecificSaveTargets: List<SaveTarget>
        get() =
            when (entry) {
                is NativePCGame -> saveTargets.forEntry(entry)
                is EmulatorSystem -> romFile?.name?.let { saveTargets.forRom(entry, it) }.orEmpty()
            }

    val effectiveCoverPath: String?
        get() =
            when (entry) {
                is NativePCGame -> entry.coverPath?.takeIf { it.isNotBlank() }
                is EmulatorSystem ->
                    romFile?.name?.let { entry.coverPathByRom[it]?.takeIf { p -> p.isNotBlank() } }
            }

    val coverFile: File?
        get() {
            // 1. Explicit individual cover path set for this game
            val customCover = effectiveCoverPath?.let { File(it) }
            if (customCover != null && customCover.exists() && customCover.isFile) {
                return customCover
            }

            val extensions = listOf("png", "jpg", "jpeg", "webp")
            val baseName = romFile?.nameWithoutExtension ?: name
            val cleanName = com.emusync.domain.library.cleanGameTitle(baseName)
            val spaceName = baseName.replace('_', ' ').trim()

            // 2. Same folder as the ROM / game with the exact same name as the ROM
            val romDir =
                romFile?.parentFile
                    ?: (entry as? NativePCGame)?.let { File(it.executablePath).parentFile }
            if (romDir != null && romDir.exists()) {
                val targetNames =
                    listOfNotNull(
                            baseName,
                            effectiveTitle,
                            cleanName,
                            spaceName,
                            romFile?.name,
                        )
                        .distinct()
                for (target in targetNames) {
                    for (ext in extensions) {
                        val candidate = File(romDir, "$target.$ext")
                        if (candidate.exists() && candidate.isFile) return candidate
                        val upperCandidate = File(romDir, "$target.${ext.uppercase()}")
                        if (upperCandidate.exists() && upperCandidate.isFile) return upperCandidate
                    }
                }
                // Case-insensitive check in same folder with exact baseName
                val sameFolderFiles = romDir.listFiles() ?: emptyArray()
                val exactMatch = sameFolderFiles.firstOrNull { f ->
                    f.isFile &&
                        extensions.any { ext -> f.extension.equals(ext, ignoreCase = true) } &&
                        f.nameWithoutExtension.equals(baseName, ignoreCase = true)
                }
                if (exactMatch != null) return exactMatch
            }

            // 3. Fallback already configured
            when (entry) {
                is EmulatorSystem -> {
                    val dirsToCheck = mutableListOf<File>()
                    val customDir =
                        entry.coversDirectory?.takeIf { it.isNotBlank() }?.let { File(it) }
                    if (customDir != null && customDir.exists()) {
                        dirsToCheck.add(customDir)
                    }

                    if (romDir != null && romDir.exists()) {
                        dirsToCheck.add(File(romDir, "covers"))
                        dirsToCheck.add(File(romDir, "boxart"))
                        dirsToCheck.add(File(romDir, "images"))
                    }

                    for (dir in dirsToCheck) {
                        // Exact lookups with clean/space variations
                        val targetNames =
                            listOfNotNull(
                                baseName,
                                cleanName.takeIf { it.isNotBlank() && it != baseName },
                                spaceName.takeIf {
                                    it.isNotBlank() && it != baseName && it != cleanName
                                },
                                romFile?.name,
                            )
                        for (target in targetNames) {
                            for (ext in extensions) {
                                val candidate = File(dir, "$target.$ext")
                                if (candidate.exists()) return candidate
                                val upperCandidate = File(dir, "$target.${ext.uppercase()}")
                                if (upperCandidate.exists()) return upperCandidate
                            }
                        }

                        // Case-insensitive fallback
                        val files = dir.listFiles() ?: emptyArray()
                        val match = files.firstOrNull { f ->
                            f.isFile &&
                                extensions.any { ext ->
                                    f.extension.equals(ext, ignoreCase = true)
                                } &&
                                (f.nameWithoutExtension.equals(baseName, ignoreCase = true) ||
                                    f.nameWithoutExtension.equals(cleanName, ignoreCase = true) ||
                                    f.nameWithoutExtension.equals(spaceName, ignoreCase = true))
                        }
                        if (match != null) return match
                    }
                }
                is NativePCGame -> {
                    val execDir = File(entry.executablePath).parentFile
                    if (execDir != null && execDir.exists()) {
                        val targetNames = listOf(name, cleanName, "cover", "boxart")
                        for (target in targetNames) {
                            for (ext in extensions) {
                                val candidate = File(execDir, "$target.$ext")
                                if (candidate.exists()) return candidate
                                val upperCandidate = File(execDir, "$target.${ext.uppercase()}")
                                if (upperCandidate.exists()) return upperCandidate
                            }
                        }
                    }
                }
            }
            return null
        }
}
