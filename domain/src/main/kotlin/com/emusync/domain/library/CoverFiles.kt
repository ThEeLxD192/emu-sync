package com.emusync.domain.library

/** Locates cover art without exposing desktop filesystem types to its consumers. */
fun interface CoverFiles {
    fun find(name: String, entry: GameEntry, romPath: String?): String?
}
