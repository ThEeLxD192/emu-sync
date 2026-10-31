package com.emusync.ui.updates

import com.emusync.domain.update.UpdateFiles
import com.emusync.domain.update.UpdateInstaller
import com.emusync.domain.update.UpdateSource

/** Update capabilities borrowed from the desktop resource owner. */
data class UpdateServices(
    val source: UpdateSource,
    val files: UpdateFiles,
    val installer: UpdateInstaller,
)
