package com.emusync.config

import com.emusync.model.GoogleDriveConfig

internal data class GoogleDriveSnapshot(val credentials: GoogleDriveConfig?, val revision: Long)
