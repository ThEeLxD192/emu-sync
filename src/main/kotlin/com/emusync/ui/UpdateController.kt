package com.emusync.ui

import com.emusync.application.update.UpdateCoordinator
import com.emusync.application.update.UpdateState
import com.emusync.domain.update.UpdateFiles
import com.emusync.domain.update.UpdateInfo
import com.emusync.domain.update.UpdateInstaller
import com.emusync.domain.update.UpdateSource
import java.io.File
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update

/** Maps the update workflow to presentation state. Resources remain owned by the composition. */
internal class UpdateController(
    private val state: MutableStateFlow<AppUiState>,
    source: UpdateSource,
    files: UpdateFiles,
    installer: UpdateInstaller,
) {
    private val updates =
        UpdateCoordinator(
            source,
            files,
            installer,
            initialState = state.value.updateState.toWorkflowState(),
        ) { next ->
            state.update { it.copy(updateState = next.toUiState()) }
        }

    suspend fun checkForUpdates(manual: Boolean) {
        if (manual && updates.canCheck) state.update { it.copy(showUpdateDialog = true) }
        updates.checkForUpdates()
    }

    suspend fun download(info: UpdateInfo) = updates.download(info)

    fun restart(file: File) = updates.restart(file.absolutePath, state.value.isOperationInProgress)
}

private fun UpdateState.toUiState(): UpdateUiState =
    when (this) {
        UpdateState.Idle -> UpdateUiState.Idle
        UpdateState.Checking -> UpdateUiState.Checking
        is UpdateState.Available -> UpdateUiState.Available(info)
        is UpdateState.Downloading -> UpdateUiState.Downloading(progress, info)
        is UpdateState.ReadyToRestart ->
            UpdateUiState.ReadyToRestart(File(downloadedPath), isSteamGameMode)
        is UpdateState.Error -> UpdateUiState.Error(message)
    }

private fun UpdateUiState.toWorkflowState(): UpdateState =
    when (this) {
        UpdateUiState.Idle -> UpdateState.Idle
        UpdateUiState.Checking -> UpdateState.Checking
        is UpdateUiState.Available -> UpdateState.Available(info)
        is UpdateUiState.Downloading -> UpdateState.Downloading(progress, info)
        is UpdateUiState.ReadyToRestart ->
            UpdateState.ReadyToRestart(downloadedFile.absolutePath, isSteamGameMode)
        is UpdateUiState.Error -> UpdateState.Error(message)
    }
