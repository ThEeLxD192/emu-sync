package com.emusync.ui

import com.emusync.drive.SyncDecision
import com.emusync.model.AppConfig
import com.emusync.model.EmulatorSystem
import com.emusync.model.GameEntry
import com.emusync.model.NativePCGame
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
 * Immutable Single Source of Truth for the entire application UI state.
 */
data class AppUiState(
    val config: AppConfig? = null,
    val selectedEntry: GameEntry? = null,
    val gameItems: List<GameItem> = emptyList(),
    val status: AppStatus = AppStatus.Idle,
    val isLoading: Boolean = false,
    val saveSetupRequest: GameItem? = null,
)
