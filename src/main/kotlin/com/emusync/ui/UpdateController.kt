package com.emusync.ui

import com.emusync.update.UpdateInfo
import com.emusync.update.UpdateManager
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.withContext
import java.io.File
import java.nio.file.Files
import java.util.concurrent.atomic.AtomicLong

/** Owns update requests so late checks cannot replace an active or downloaded update. */
internal class UpdateController(private val state: MutableStateFlow<AppUiState>, private val manager: UpdateManager) {
    private val downloadMutex = Mutex()
    private val generation = AtomicLong()

    suspend fun checkForUpdates(manual: Boolean) {
        if (downloadMutex.isLocked || state.value.updateState is UpdateUiState.ReadyToRestart) return
        val request = generation.incrementAndGet()
        val previous = state.value.updateState.takeUnless { it is UpdateUiState.Checking } ?: UpdateUiState.Idle
        state.update { it.copy(updateState = UpdateUiState.Checking, showUpdateDialog = it.showUpdateDialog || manual) }
        try {
            val info = manager.checkForUpdates()
            state.update {
                if (generation.get() != request || downloadMutex.isLocked) it
                else it.copy(updateState = info?.let(UpdateUiState::Available) ?: UpdateUiState.Idle)
            }
        } catch (cancelled: CancellationException) {
            state.update { if (generation.get() == request && !downloadMutex.isLocked) it.copy(updateState = previous) else it }
            throw cancelled
        }
    }

    suspend fun download(info: UpdateInfo) {
        if (state.value.updateState is UpdateUiState.ReadyToRestart) return
        if (!downloadMutex.tryLock()) return
        generation.incrementAndGet()
        var destination: File? = null
        var keepDownload = false
        state.update { it.copy(updateState = UpdateUiState.Downloading(0f, info)) }
        try {
            val windows = manager.isWindows()
            val extension = File(info.assetName).extension.ifBlank { if (windows) "msi" else "AppImage" }
            destination = withContext(Dispatchers.IO) {
                val currentParent = if (!windows) manager.getCurrentAppImagePath()?.let { File(it).absoluteFile.parentFile } else null
                val directory = currentParent?.takeIf { it.canWrite() } ?: File(System.getProperty("java.io.tmpdir"))
                Files.createTempFile(directory.toPath(), ".EmuSync-update-", ".$extension").toFile()
            }
            manager.downloadUpdate(info.downloadUrl, destination) { progress ->
                state.update { it.copy(updateState = UpdateUiState.Downloading(progress, info)) }
            }.getOrThrow()
            check(info.assetSize <= 0 || destination.length() == info.assetSize) {
                "The update size does not match the release: expected ${info.assetSize} bytes, received ${destination.length()}"
            }
            keepDownload = true
            state.update { it.copy(updateState = UpdateUiState.ReadyToRestart(destination, manager.isSteamGameMode())) }
        } catch (cancelled: CancellationException) {
            state.update { it.copy(updateState = UpdateUiState.Available(info)) }
            throw cancelled
        } catch (failure: Exception) {
            state.update { it.copy(updateState = UpdateUiState.Error("Failed to download update: ${failure.message ?: "Connection error"}")) }
        } finally {
            if (!keepDownload) destination?.delete()
            downloadMutex.unlock()
        }
    }

    fun restart(file: File): Boolean {
        if (state.value.isOperationInProgress || downloadMutex.isLocked) return false
        val success = manager.applyUpdateAndRestart(file)
        if (!success) state.update { it.copy(updateState = UpdateUiState.Error(
            "Could not restart automatically. The update file is saved at: ${file.absolutePath}")) }
        return success
    }
}
