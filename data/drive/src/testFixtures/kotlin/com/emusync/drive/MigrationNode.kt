package com.emusync.drive

data class MigrationNode(
    val id: String,
    var name: String,
    var parent: String?,
    val folder: Boolean,
    val content: String = "SAVE DATA",
)
