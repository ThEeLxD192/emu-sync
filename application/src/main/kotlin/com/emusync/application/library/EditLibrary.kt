package com.emusync.application.library

import com.emusync.domain.config.AppConfig
import com.emusync.domain.config.ConfigurationStore
import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.SteamLibrary
import com.emusync.domain.operation.recoverOperationFailure
import com.emusync.domain.sync.SaveSynchronizerFactory
import com.emusync.domain.sync.SyncEvent
import com.emusync.domain.sync.SyncGame

/** Persists edits only after cloud migration succeeds, retaining unrelated concurrent edits. */
class EditLibrary(
    private val store: ConfigurationStore,
    private val synchronizers: SaveSynchronizerFactory,
    private val steam: SteamLibrary,
    private val onStatus: (SyncEvent) -> Unit,
) {
    fun reorderEntries(config: AppConfig, fromIndex: Int, toIndex: Int): AppConfig {
        val entries = config.entries.toMutableList()
        if (fromIndex !in entries.indices || toIndex !in entries.indices || fromIndex == toIndex)
            return config
        entries.add(toIndex, entries.removeAt(fromIndex))
        return reorderEntriesList(entries)
    }

    fun reorderEntriesList(newEntries: List<GameEntry>): AppConfig = store.updateConfig { current ->
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

    fun addEntry(config: AppConfig, entry: GameEntry): AppConfig? {
        if (entry.name.isBlank() || config.entries.any { it.name == entry.name }) {
            onStatus(
                SyncEvent.Error("Choose a non-empty category name that is not already in use.")
            )
            return null
        }
        return store.updateConfig { it.copy(entries = it.entries + entry) }
    }

    fun deleteEntry(entry: GameEntry): AppConfig = store.updateConfig { current ->
        current.copy(entries = current.entries.filter { it.name != entry.name })
    }

    suspend fun editEntry(
        config: AppConfig,
        oldEntry: GameEntry,
        newEntry: GameEntry,
    ): EntryEditResult? {
        if (!validateEdit(config, oldEntry, newEntry)) return null
        val edited = preserveSharedCloudRoot(config, oldEntry, newEntry)
        return if (migrateEntry(config, oldEntry, edited)) {
            val updated = store.updateConfig { current ->
                current.copy(
                    entries = current.entries.map { if (it.name == oldEntry.name) edited else it }
                )
            }
            refreshSteamLauncher(edited)
            EntryEditResult(updated, edited)
        } else null
    }

    suspend fun editGameOverride(
        config: AppConfig,
        game: SyncGame,
        changes: GameOverrideChanges,
    ): EntryEditResult? {
        val latestEntry = config.entries.find { it.name == game.entry.name } ?: return null
        val current = game.copy(entry = latestEntry)
        val edited = buildGameEntry(current, changes)
        return if (edited != null && migrateGameTitle(config, current, edited, changes))
            editEntry(config, current.entry, edited)
        else null
    }

    private fun validateEdit(config: AppConfig, oldEntry: GameEntry, newEntry: GameEntry): Boolean =
        when {
            config.entries.none { it.name == oldEntry.name } -> false
            newEntry.name.isBlank() ||
                config.entries.any { it.name != oldEntry.name && it.name == newEntry.name } -> {
                onStatus(
                    SyncEvent.Error("Choose a non-empty category name that is not already in use.")
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
        if (config.cloudCredentials == null) true
        else synchronizers.create(config, onStatus).renameEntryCloudData(oldEntry, edited)

    private fun refreshSteamLauncher(entry: GameEntry) {
        if (entry.steamAppId != null)
            recoverOperationFailure(
                action = {
                    steam.register(entry)
                    Unit
                },
                recover = { Unit },
            )
    }

    private suspend fun migrateGameTitle(
        config: AppConfig,
        current: SyncGame,
        edited: GameEntry,
        changes: GameOverrideChanges,
    ): Boolean {
        val oldTitle = current.effectiveTitle
        val romName =
            current.romPath?.let { java.nio.file.Path.of(it).fileName.toString() } ?: current.name
        val newTitle =
            (edited as? EmulatorSystem)?.getEffectiveTitle(romName, current.name) ?: edited.name
        return if (
            newTitle == oldTitle || edited !is EmulatorSystem || config.cloudCredentials == null
        )
            true
        else
            synchronizers
                .create(config, onStatus)
                .renameGameCloudData(
                    entry = edited,
                    romName = romName,
                    oldTitle = oldTitle,
                    newTitle = newTitle,
                    localSaveTargets = changes.targets.ifEmpty { current.effectiveSaveTargets },
                )
    }
}
