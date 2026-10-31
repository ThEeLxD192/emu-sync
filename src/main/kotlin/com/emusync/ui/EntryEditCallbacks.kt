package com.emusync.ui

import com.emusync.domain.library.GameEntry

internal data class EntryEditCallbacks(
    val selectEntry: suspend (GameEntry) -> Unit,
    val clearSelection: () -> Unit,
    val publishStatus: (AppStatus) -> Unit,
)
