package com.emusync.domain.update

interface UpdateFiles {
    suspend fun prepareDestination(info: UpdateInfo): String

    fun size(path: String): Long

    fun delete(path: String)
}
