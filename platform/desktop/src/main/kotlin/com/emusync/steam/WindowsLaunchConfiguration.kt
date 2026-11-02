package com.emusync.steam

import kotlinx.serialization.Serializable

@Serializable
internal data class WindowsLaunchConfiguration(
    val executable: String,
    val signalFile: String,
    val arguments: List<String>,
)
