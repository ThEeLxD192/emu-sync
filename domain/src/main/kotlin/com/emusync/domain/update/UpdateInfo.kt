package com.emusync.domain.update

data class UpdateInfo(
    val version: String,
    val tagName: String,
    val releaseNotes: String,
    val downloadUrl: String,
    val assetSize: Long,
    val assetName: String = "",
)
