package com.emusync.ui

import com.emusync.update.UpdateInfo
import java.io.File

/** UI State for in-app updates. */
sealed interface UpdateUiState {
    data object Idle : UpdateUiState

    data object Checking : UpdateUiState

    data class Available(val info: UpdateInfo) : UpdateUiState

    data class Downloading(val progress: Float, val info: UpdateInfo) : UpdateUiState

    data class ReadyToRestart(val downloadedFile: File, val isSteamGameMode: Boolean = false) :
        UpdateUiState

    data class Error(val message: String) : UpdateUiState
}
