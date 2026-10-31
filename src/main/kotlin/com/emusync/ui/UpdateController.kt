package com.emusync.ui

import com.emusync.update.UpdateInfo
import com.emusync.update.UpdateManager
import java.io.File
import java.nio.file.Files
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.withContext

/** Owns update requests so late checks cannot replace an active or downloaded update. */
internal class UpdateController(
    private val state: MutableStateFlow<AppUiState>,
    private val manager: UpdateManager,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
) {
    private val downloadMutex = Mutex()
    private val generation = AtomicLong()

    suspend fun checkForUpdates(manual: Boolean) {
        if (downloadMutex.isLocked || state.value.updateState is UpdateUiState.ReadyToRestart)
            return
        val request = generation.incrementAndGet()
        val previous =
            state.value.updateState.takeUnless { it is UpdateUiState.Checking }
                ?: UpdateUiState.Idle
        state.update {
            it.copy(
                updateState = UpdateUiState.Checking,
                showUpdateDialog = it.showUpdateDialog || manual,
            )
        }
        restoreUiOnCancellation(
            action = {
                val info = manager.checkForUpdates()
                state.update {
                    if (generation.get() != request || downloadMutex.isLocked) it
                    else
                        it.copy(
                            updateState = info?.let(UpdateUiState::Available) ?: UpdateUiState.Idle
                        )
                }
            },
            restore = {
                state.update {
                    if (generation.get() == request && !downloadMutex.isLocked)
                        it.copy(updateState = previous)
                    else it
                }
            },
        )
    }

    suspend fun download(info: UpdateInfo) {
        if (state.value.updateState is UpdateUiState.ReadyToRestart) return
        if (!downloadMutex.tryLock()) return
        generation.incrementAndGet()
        var destination: File? = null
        var keepDownload = false
        state.update { it.copy(updateState = UpdateUiState.Downloading(0f, info)) }
        try {
            restoreUiOnCancellation(
                action = {
                    recoverUiFailure(
                        action = {
                            val downloaded = prepareDestination(info)
                            destination = downloaded
                            manager
                                .downloadUpdate(info.downloadUrl, downloaded) { progress ->
                                    state.update {
                                        it.copy(
                                            updateState = UpdateUiState.Downloading(progress, info)
                                        )
                                    }
                                }
                                .getOrThrow()
                            check(info.assetSize <= 0 || downloaded.length() == info.assetSize) {
                                "The update size does not match the release: expected ${info.assetSize} bytes, received ${downloaded.length()}"
                            }
                            keepDownload = true
                            state.update {
                                it.copy(
                                    updateState =
                                        UpdateUiState.ReadyToRestart(
                                            downloaded,
                                            manager.isSteamGameMode(),
                                        )
                                )
                            }
                        },
                        recover = { failure ->
                            state.update {
                                it.copy(
                                    updateState =
                                        UpdateUiState.Error(
                                            "Failed to download update: ${failure.message ?: "Connection error"}"
                                        )
                                )
                            }
                        },
                    )
                },
                restore = { state.update { it.copy(updateState = UpdateUiState.Available(info)) } },
            )
        } finally {
            if (!keepDownload) destination?.delete()
            downloadMutex.unlock()
        }
    }

    private suspend fun prepareDestination(info: UpdateInfo): File {
        val windows = manager.isWindows()
        val extension =
            File(info.assetName).extension.ifBlank { if (windows) "msi" else "AppImage" }
        return withContext(ioDispatcher) {
            val currentParent =
                if (!windows)
                    manager.getCurrentAppImagePath()?.let {
                        File(it).absoluteFile.parentFile
                    }
                else null
            val directory =
                currentParent?.takeIf { it.canWrite() }
                    ?: File(System.getProperty("java.io.tmpdir"))
            Files.createTempFile(directory.toPath(), ".EmuSync-update-", ".$extension").toFile()
        }
    }

    fun restart(file: File): Boolean {
        if (state.value.isOperationInProgress || downloadMutex.isLocked) return false
        val success = manager.applyUpdateAndRestart(file)
        if (!success)
            state.update {
                it.copy(
                    updateState =
                        UpdateUiState.Error(
                            "Could not restart automatically. The update file is saved at: ${file.absolutePath}"
                        )
                )
            }
        return success
    }
}
