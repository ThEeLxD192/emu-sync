package com.emusync.ui

import com.emusync.config.ConfigManager
import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.model.AppConfig
import com.emusync.steam.SteamShortcutManager
import io.ktor.client.HttpClient
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update

internal data class EntryEditCallbacks(
    val selectEntry: suspend (GameEntry) -> Unit,
    val clearSelection: () -> Unit,
    val publishStatus: (AppStatus) -> Unit,
)

/** Persists edits only after cloud migration succeeds, retaining unrelated concurrent edits. */
internal class EntryEditor(
    private val state: MutableStateFlow<AppUiState>,
    private val configManager: ConfigManager,
    private val client: HttpClient,
    private val steamManager: SteamShortcutManager,
    private val callbacks: EntryEditCallbacks,
) {
    fun reorderEntries(fromIndex: Int, toIndex: Int) {
        val cfg = state.value.config ?: return
        val currentEntries = cfg.entries.toMutableList()
        if (
            fromIndex !in currentEntries.indices ||
                toIndex !in currentEntries.indices ||
                fromIndex == toIndex
        ) {
            return
        }

        val movedItem = currentEntries.removeAt(fromIndex)
        currentEntries.add(toIndex, movedItem)

        val updatedConfig = configManager.update { current ->
            val byName = current.entries.associateBy { it.name }
            val ordered = currentEntries.mapNotNull { byName[it.name] }
            current.copy(
                entries =
                    ordered +
                        current.entries.filter { candidate ->
                            ordered.none { it.name == candidate.name }
                        }
            )
        }
        state.update { it.copy(config = updatedConfig) }
    }

    fun reorderEntriesList(newEntries: List<GameEntry>) {
        val cfg = state.value.config ?: return
        if (cfg.entries == newEntries) return

        val updatedConfig = configManager.update { current ->
            val byName = current.entries.associateBy { it.name }
            val ordered = newEntries.mapNotNull { byName[it.name] }.distinctBy { it.name }
            current.copy(
                entries =
                    ordered +
                        current.entries.filter { candidate ->
                            ordered.none { it.name == candidate.name }
                        }
            )
        }
        state.update { it.copy(config = updatedConfig) }
    }

    suspend fun addEntry(entry: GameEntry) {
        if (state.value.config == null) return
        if (
            entry.name.isBlank() ||
                state.value.config?.entries?.any { it.name == entry.name } == true
        ) {
            callbacks.publishStatus(
                AppStatus.Error("Choose a non-empty category name that is not already in use.")
            )
            return
        }
        val updatedConfig = configManager.update { it.copy(entries = it.entries + entry) }
        state.update { it.copy(config = updatedConfig) }
        callbacks.selectEntry(entry)
    }

    fun deleteEntry(entry: GameEntry) {
        if (state.value.config == null) return
        val updatedConfig = configManager.update { current ->
            current.copy(entries = current.entries.filter { it.name != entry.name })
        }
        state.update { it.copy(config = updatedConfig) }
        if (state.value.selectedEntry === entry || state.value.selectedEntry?.name == entry.name) {
            callbacks.clearSelection()
        }
    }

    suspend fun editEntry(oldEntry: GameEntry, newEntry: GameEntry): Boolean {
        val config = state.value.config ?: return false
        return if (validateEdit(config, oldEntry, newEntry)) {
            val edited = preserveSharedCloudRoot(config, oldEntry, newEntry)
            if (migrateEntry(config, oldEntry, edited)) {
                publishEditedEntry(oldEntry, edited)
                refreshSteamLauncher(edited)
                true
            } else false
        } else false
    }

    suspend fun editGameOverride(game: GameItem, changes: GameOverrideChanges): Boolean {
        val latestEntry =
            state.value.config?.entries?.find { it.name == game.entry.name } ?: return false
        val current = game.copy(entry = latestEntry)
        val edited = buildGameEntry(current, changes)
        return if (edited != null && migrateGameTitle(current, edited, changes))
            editEntry(current.entry, edited)
        else false
    }

    private fun validateEdit(config: AppConfig, oldEntry: GameEntry, newEntry: GameEntry): Boolean =
        when {
            config.entries.none { it.name == oldEntry.name } -> false
            newEntry.name.isBlank() ||
                config.entries.any { it.name != oldEntry.name && it.name == newEntry.name } -> {
                callbacks.publishStatus(
                    AppStatus.Error("Choose a non-empty category name that is not already in use.")
                )
                false
            }
            else -> true
        }

    private suspend fun migrateEntry(
        config: AppConfig,
        oldEntry: GameEntry,
        edited: GameEntry,
    ): Boolean =
        if (config.googleDrive == null) true
        else
            createSaveSynchronizer(client, config, configManager, callbacks.publishStatus)
                .renameEntryCloudData(oldEntry, edited)

    private suspend fun publishEditedEntry(oldEntry: GameEntry, edited: GameEntry) {
        val updated = configManager.update { current ->
            current.copy(
                entries = current.entries.map { if (it.name == oldEntry.name) edited else it }
            )
        }
        state.update { it.copy(config = updated) }
        if (
            state.value.selectedEntry === oldEntry ||
                state.value.selectedEntry?.name == oldEntry.name
        )
            callbacks.selectEntry(edited)
    }

    private fun refreshSteamLauncher(entry: GameEntry) {
        if (entry.steamAppId != null)
            recoverUiFailure(
                action = {
                    steamManager.registerEntry(entry)
                    Unit
                },
                recover = { Unit },
            )
    }

    private suspend fun migrateGameTitle(
        current: GameItem,
        edited: GameEntry,
        changes: GameOverrideChanges,
    ): Boolean {
        val oldTitle = current.effectiveTitle
        val newTitle = current.copy(entry = edited).effectiveTitle
        val config = state.value.config
        return if (newTitle == oldTitle || edited !is EmulatorSystem || config?.googleDrive == null)
            true
        else
            createSaveSynchronizer(client, config, configManager, callbacks.publishStatus)
                .renameGameCloudData(
                    entry = edited,
                    romName = current.romFile?.name ?: current.name,
                    oldTitle = oldTitle,
                    newTitle = newTitle,
                    localSaveTargets = changes.targets.ifEmpty { current.effectiveSaveTargets },
                )
    }
}
