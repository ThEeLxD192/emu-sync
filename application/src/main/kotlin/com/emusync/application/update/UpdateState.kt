package com.emusync.application.update

import com.emusync.domain.update.UpdateInfo

/** Observable update workflow, independent of visual state and local file objects. */
sealed interface UpdateState {
    data object Idle : UpdateState

    data object Checking : UpdateState

    data class Available(val info: UpdateInfo) : UpdateState

    data class Downloading(val progress: Float, val info: UpdateInfo) : UpdateState

    data class ReadyToRestart(val downloadedPath: String, val isSteamGameMode: Boolean = false) :
        UpdateState

    data class Error(val message: String) : UpdateState
}
