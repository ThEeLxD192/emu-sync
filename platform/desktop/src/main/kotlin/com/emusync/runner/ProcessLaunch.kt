package com.emusync.runner

internal data class ProcessLaunch(
    val name: String,
    val existingIds: Set<Long>,
    val request: Process?,
    val desktopHandoffSucceeded: Boolean,
)
