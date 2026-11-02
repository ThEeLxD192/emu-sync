package com.emusync.domain.sync

/** Provider-independent identity and evidence observed for one remote save. */
data class CloudSaveState(
    val id: String,
    val modifiedTime: String,
    val sha256Checksum: String? = null,
    val version: String? = null,
)
