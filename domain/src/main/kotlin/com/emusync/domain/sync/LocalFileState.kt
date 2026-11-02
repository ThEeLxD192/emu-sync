package com.emusync.domain.sync

/** Immutable metadata used to plan a sync without performing file or network I/O. */
data class LocalFileState(val relativePath: String, val modifiedTimeMs: Long)
