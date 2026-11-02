package com.emusync.application.update

import com.emusync.application.operation.restoreOnCancellation
import com.emusync.domain.operation.recoverOperationFailure
import com.emusync.domain.update.UpdateFiles
import com.emusync.domain.update.UpdateInfo
import com.emusync.domain.update.UpdateInstaller
import com.emusync.domain.update.UpdateSource
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex

/** Owns update requests so late checks cannot replace an active or downloaded update. */
class UpdateCoordinator(
    private val source: UpdateSource,
    private val files: UpdateFiles,
    private val installer: UpdateInstaller,
    initialState: UpdateState = UpdateState.Idle,
    private val onState: (UpdateState) -> Unit,
) {
    private val state = MutableStateFlow(initialState)

    private fun publish(transform: (UpdateState) -> UpdateState) {
        val previous = state.value
        state.update(transform)
        if (state.value !== previous) onState(state.value)
    }

    private val downloadMutex = Mutex()
    private val generation = AtomicLong()

    val canCheck: Boolean
        get() = !downloadMutex.isLocked && state.value !is UpdateState.ReadyToRestart

    suspend fun checkForUpdates() {
        if (!canCheck) return
        val request = generation.incrementAndGet()
        val previous = state.value.takeUnless { it is UpdateState.Checking } ?: UpdateState.Idle
        publish {
            UpdateState.Checking
        }
        restoreOnCancellation(
            action = {
                val info = source.check()
                publish {
                    if (generation.get() != request || downloadMutex.isLocked) it
                    else info?.let(UpdateState::Available) ?: UpdateState.Idle
                }
            },
            restore = {
                publish {
                    if (generation.get() == request && !downloadMutex.isLocked) previous else it
                }
            },
        )
    }

    suspend fun download(info: UpdateInfo) {
        if (state.value is UpdateState.ReadyToRestart) return
        if (!downloadMutex.tryLock()) return
        generation.incrementAndGet()
        var destination: String? = null
        var keepDownload = false
        publish { UpdateState.Downloading(0f, info) }
        try {
            restoreOnCancellation(
                action = {
                    recoverOperationFailure(
                        action = {
                            val downloaded = files.prepareDestination(info)
                            destination = downloaded
                            source
                                .download(info, downloaded) { progress ->
                                    publish {
                                        UpdateState.Downloading(progress, info)
                                    }
                                }
                                .getOrThrow()
                            check(info.assetSize <= 0 || files.size(downloaded) == info.assetSize) {
                                "The update size does not match the release: expected ${info.assetSize} bytes, received ${files.size(downloaded)}"
                            }
                            keepDownload = true
                            publish {
                                UpdateState.ReadyToRestart(downloaded, installer.isSteamGameMode())
                            }
                        },
                        recover = { failure ->
                            publish {
                                UpdateState.Error(
                                    "Failed to download update: ${failure.message ?: "Connection error"}"
                                )
                            }
                        },
                    )
                },
                restore = { publish { UpdateState.Available(info) } },
            )
        } finally {
            if (!keepDownload) destination?.let(files::delete)
            downloadMutex.unlock()
        }
    }

    fun restart(path: String, operationInProgress: Boolean): Boolean {
        if (operationInProgress || downloadMutex.isLocked) return false
        val success = installer.restart(path)
        if (!success)
            publish {
                UpdateState.Error(
                    "Could not restart automatically. The update file is saved at: $path"
                )
            }
        return success
    }
}
