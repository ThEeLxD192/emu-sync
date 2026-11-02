package com.emusync.config.dto

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
internal enum class SaveTargetTypeDto {
    @SerialName("file") FILE,
    @SerialName("directory") DIRECTORY,
    /** The legacy path did not exist during migration, so its type was unknowable. */
    @SerialName("unknown") UNKNOWN,
}
