package com.emusync.drive

import java.io.File

/** Identifies both copies of one save and its exact remote destination. */
internal data class PlannedSave(
    val local: File,
    val remote: DriveFile?,
    val folderId: String,
    val driveName: String,
)
