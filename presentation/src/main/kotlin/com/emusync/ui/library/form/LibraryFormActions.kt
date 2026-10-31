package com.emusync.ui.library.form

import com.emusync.domain.execution.PickerType
import com.emusync.domain.library.SaveTargetAccess

/** Platform callbacks supplied by the desktop composition to the shared forms. */
class LibraryFormActions(
    val saveTargets: SaveTargetAccess,
    val pickPath: suspend (String, String, PickerType) -> String?,
    val detectFullscreenArgs: (String) -> String,
)
