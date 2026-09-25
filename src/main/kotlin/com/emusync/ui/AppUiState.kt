package com.emusync.ui

import com.emusync.drive.SyncDecision
import com.emusync.model.AppConfig
import com.emusync.model.EmulatorSystem
import com.emusync.model.GameEntry
import com.emusync.model.NativePCGame
import com.emusync.update.UpdateInfo
import java.io.File

/**
 * Represents a displayable game item in the grid.
 * Unifies ROMs (from scanning) and native games into a single UI model.
 */
data class GameItem(
    val name: String,
    val entry: GameEntry,
    val romFile: File? = null,
) {
    val effectiveSavePaths: List<String>
        get() = when (entry) {
            is NativePCGame -> entry.savePaths
            is EmulatorSystem -> {
                val romName = romFile?.name
                val romPaths = if (romName != null) entry.savePathsByRom[romName] else null
                
                if (!romPaths.isNullOrEmpty()) {
                    romPaths
                } else if (entry.savePaths.isNotEmpty()) {
                    entry.savePaths
                } else {
                    emptyList()
                }
            }
        }

    val effectiveCoverPath: String?
        get() = when (entry) {
            is NativePCGame -> entry.coverPath?.takeIf { it.isNotBlank() }
            is EmulatorSystem -> romFile?.name?.let { entry.coverPathByRom[it]?.takeIf { p -> p.isNotBlank() } }
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
            val cleanName = baseName.replace(Regex("\\s*[\\[\\(].*?[\\]\\)]"), "").trim()
            val spaceName = baseName.replace('_', ' ').trim()

            // 2. Same folder as the ROM / game with the exact same name as the ROM
            val romDir = romFile?.parentFile ?: (entry as? NativePCGame)?.let { File(it.executablePath).parentFile }
            if (romDir != null && romDir.exists()) {
                val targetNames = listOfNotNull(
                    baseName,
                    romFile?.name
                )
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
                    f.isFile && extensions.any { ext -> f.extension.equals(ext, ignoreCase = true) } &&
                        f.nameWithoutExtension.equals(baseName, ignoreCase = true)
                }
                if (exactMatch != null) return exactMatch
            }

            // 3. Fallback already configured
            when (entry) {
                is EmulatorSystem -> {
                    val dirsToCheck = mutableListOf<File>()
                    val customDir = entry.coversDirectory?.takeIf { it.isNotBlank() }?.let { File(it) }
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
                        val targetNames = listOfNotNull(
                            baseName,
                            cleanName.takeIf { it.isNotBlank() && it != baseName },
                            spaceName.takeIf { it.isNotBlank() && it != baseName && it != cleanName },
                            romFile?.name
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
                            f.isFile && extensions.any { ext -> f.extension.equals(ext, ignoreCase = true) } &&
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

/**
 * Current status of the game launch/sync lifecycle.
 */
sealed interface AppStatus {
    data object Idle : AppStatus
    data object Loading : AppStatus
    data class Syncing(val message: String) : AppStatus
    data object Playing : AppStatus
    data class Error(val message: String) : AppStatus
    data class Conflict(
        val localDate: String,
        val cloudDate: String,
        val gameName: String,
        val onResolve: (SyncDecision) -> Unit,
    ) : AppStatus
}

/**
 * Cloud sync status for an entry/emulator system (Steam Cloud style).
 */
enum class CloudSyncStatus {
    IDLE,
    CHECKING,
    IN_SYNC,
    OUT_OF_SYNC,
    SYNCING,
    CONFLICT,
    NOT_CONFIGURED,
    ERROR,
}

/**
 * UI State for in-app updates.
 */
sealed interface UpdateUiState {
    data object Idle : UpdateUiState
    data object Checking : UpdateUiState
    data class Available(val info: UpdateInfo) : UpdateUiState
    data class Downloading(val progress: Float, val info: UpdateInfo) : UpdateUiState
    data class ReadyToRestart(val downloadedFile: File, val isSteamGameMode: Boolean = false) : UpdateUiState
    data class Error(val message: String) : UpdateUiState
}

/**
 * Immutable Single Source of Truth for the entire application UI state.
 */
data class AppUiState(
    val config: AppConfig? = null,
    val selectedEntry: GameEntry? = null,
    val gameItems: List<GameItem> = emptyList(),
    val status: AppStatus = AppStatus.Idle,
    val isLoading: Boolean = false,
    val saveSetupRequest: GameItem? = null,
    val entrySyncStatus: Map<String, CloudSyncStatus> = emptyMap(),
    val updateState: UpdateUiState = UpdateUiState.Idle,
    val showUpdateDialog: Boolean = false,
)
