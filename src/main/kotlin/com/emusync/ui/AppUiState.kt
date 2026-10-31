package com.emusync.ui

import com.emusync.domain.library.GameEntry
import com.emusync.model.AppConfig

/** Immutable Single Source of Truth for the entire application UI state. */
data class AppUiState(
    val config: AppConfig? = null,
    val selectedEntry: GameEntry? = null,
    val gameItems: List<GameItem> = emptyList(),
    val status: AppStatus = AppStatus.Idle,
    val isLoading: Boolean = false,
    val isOperationInProgress: Boolean = false,
    val saveSetupRequest: SaveSetupRequest? = null,
    val entrySyncStatus: Map<String, CloudSyncStatus> = emptyMap(),
    val updateState: UpdateUiState = UpdateUiState.Idle,
    val showUpdateDialog: Boolean = false,
)
