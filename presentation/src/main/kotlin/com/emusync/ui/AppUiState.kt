package com.emusync.ui

import com.emusync.domain.config.AppConfig
import com.emusync.domain.library.GameEntry
import com.emusync.ui.library.GameItem
import com.emusync.ui.library.sync.CloudSyncStatus
import com.emusync.ui.library.sync.SaveSetupRequest
import com.emusync.ui.updates.UpdateUiState

/** Immutable Single Source of Truth for the entire application UI state. */
data class AppUiState(
    val config: AppConfig? = null,
    val selectedEntry: GameEntry? = null,
    val steamAvailable: Boolean = false,
    val steamRegisteredEntries: Set<String> = emptySet(),
    val gameItems: List<GameItem> = emptyList(),
    val status: AppStatus = AppStatus.Idle,
    val isLoading: Boolean = false,
    val isOperationInProgress: Boolean = false,
    val saveSetupRequest: SaveSetupRequest? = null,
    val entrySyncStatus: Map<String, CloudSyncStatus> = emptyMap(),
    val updateState: UpdateUiState = UpdateUiState.Idle,
    val showUpdateDialog: Boolean = false,
)
