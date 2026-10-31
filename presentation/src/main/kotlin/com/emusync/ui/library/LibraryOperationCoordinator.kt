package com.emusync.ui.library

import com.emusync.application.operation.OperationCoordinator
import com.emusync.application.operation.OperationEvent
import com.emusync.ui.AppStatus
import com.emusync.ui.AppUiState
import com.emusync.ui.library.sync.CloudSyncStatus
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update

/** Translates operation lifecycle into visual busy and cancellation state. */
internal class LibraryOperationCoordinator(state: MutableStateFlow<AppUiState>) {
    val operations = OperationCoordinator { event ->
        state.update { current ->
            when (event) {
                OperationEvent.STARTED ->
                    current.copy(
                        isOperationInProgress = true,
                        entrySyncStatus =
                            current.entrySyncStatus.mapValues { (_, status) ->
                                if (status == CloudSyncStatus.CHECKING) CloudSyncStatus.IDLE
                                else status
                            },
                    )
                OperationEvent.CANCELLED ->
                    current.copy(
                        status = AppStatus.Idle,
                        entrySyncStatus =
                            current.entrySyncStatus.mapValues { (_, status) ->
                                if (status == CloudSyncStatus.SYNCING) CloudSyncStatus.IDLE
                                else status
                            },
                    )
                OperationEvent.FINISHED -> current.copy(isOperationInProgress = false)
            }
        }
    }
    val revision
        get() = operations.revision

    val isBusy
        get() = operations.isBusy

    suspend fun <T> runIfIdle(block: suspend () -> T): T? = operations.runIfIdle(block)
}
