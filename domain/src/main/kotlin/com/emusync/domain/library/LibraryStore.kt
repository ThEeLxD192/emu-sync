package com.emusync.domain.library

/** Updates the latest library atomically, retaining unrelated persisted settings. */
interface LibraryStore {
    fun loadEntries(): List<GameEntry>

    fun updateEntries(transform: (List<GameEntry>) -> List<GameEntry>): List<GameEntry>
}
