package com.emusync.ui.library

import com.emusync.domain.library.GameEntry
import com.emusync.ui.AppStatus

internal data class EntryEditCallbacks(
    val selectEntry: suspend (GameEntry) -> Unit,
    val clearSelection: () -> Unit,
    val publishStatus: (AppStatus) -> Unit,
)
