package com.emusync.ui.library

import com.emusync.application.library.EditLibrary
import com.emusync.application.library.EntryEditResult
import com.emusync.application.library.GameOverrideChanges
import com.emusync.domain.config.ConfigurationStore
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.SaveTargetAccess
import com.emusync.domain.library.SteamLibrary
import com.emusync.domain.sync.SaveSynchronizer
import com.emusync.domain.sync.SaveSynchronizerFactory
import com.emusync.domain.sync.SyncEvent
import com.emusync.ui.AppStatus
import com.emusync.ui.AppUiState
import com.emusync.ui.library.sync.toSyncGame
import com.emusync.ui.library.sync.toUiStatus
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update

/** Maps persisted library changes and neutral synchronization events to selection and UI state. */
internal class EntryEditor(
    private val state: MutableStateFlow<AppUiState>,
    configManager: ConfigurationStore,
    synchronizers: SaveSynchronizerFactory,
    steam: SteamLibrary,
    private val callbacks: EntryEditCallbacks,
    private val saveTargets: SaveTargetAccess,
) {
    private val edits =
        EditLibrary(
            configManager,
            SaveSynchronizerFactory { config, onStatus ->
                // Conflict callbacks stay at the presentation boundary.
                lateinit var sync: SaveSynchronizer
                sync =
                    synchronizers.create(config) { event ->
                        callbacks.publishStatus(event.toUiStatus(sync))
                        if (event !is SyncEvent.Error) onStatus(event)
                    }
                sync
            },
            steam,
        ) { event ->
            if (event is SyncEvent.Error) callbacks.publishStatus(AppStatus.Error(event.message))
        }

    fun reorderEntries(fromIndex: Int, toIndex: Int) {
        val config = state.value.config ?: return
        val updated = edits.reorderEntries(config, fromIndex, toIndex)
        state.update { it.copy(config = updated) }
    }

    fun reorderEntriesList(entries: List<GameEntry>) {
        if (state.value.config == null || state.value.config?.entries == entries) return
        val updated = edits.reorderEntriesList(entries)
        state.update { it.copy(config = updated) }
    }

    suspend fun addEntry(entry: GameEntry) {
        val config = state.value.config ?: return
        val updated = edits.addEntry(config, entry) ?: return
        state.update { it.copy(config = updated) }
        callbacks.selectEntry(entry)
    }

    fun deleteEntry(entry: GameEntry) {
        if (state.value.config == null) return
        val updated = edits.deleteEntry(entry)
        state.update { it.copy(config = updated) }
        if (state.value.selectedEntry?.name == entry.name) callbacks.clearSelection()
    }

    suspend fun editEntry(oldEntry: GameEntry, newEntry: GameEntry): Boolean {
        val config = state.value.config ?: return false
        return publish(oldEntry, edits.editEntry(config, oldEntry, newEntry))
    }

    suspend fun editGameOverride(game: GameItem, changes: GameOverrideChanges): Boolean {
        val config = state.value.config ?: return false
        val latest = config.entries.find { it.name == game.entry.name }
        return if (latest == null) false
        else
            publish(
                latest,
                edits.editGameOverride(
                    config,
                    game.copy(entry = latest).toSyncGame(saveTargets),
                    changes,
                ),
            )
    }

    suspend fun publish(oldEntry: GameEntry, result: EntryEditResult?): Boolean {
        if (result == null) return false
        state.update { it.copy(config = result.config) }
        if (state.value.selectedEntry?.name == oldEntry.name) callbacks.selectEntry(result.entry)
        return true
    }
}
