package com.emusync.ui.dialogs

import com.emusync.desktop.picker.FilePicker
import com.emusync.domain.execution.PickerType
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Runs blocking desktop pickers away from the dialog's UI coroutine. */
internal suspend fun pickDialogPath(
    label: String,
    initialPath: String,
    type: PickerType,
    ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
): String? = withContext(ioDispatcher) { FilePicker.pickPath(label, initialPath, type) }
