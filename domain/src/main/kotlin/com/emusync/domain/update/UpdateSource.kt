package com.emusync.domain.update

interface UpdateSource {
    suspend fun check(): UpdateInfo?

    suspend fun download(
        info: UpdateInfo,
        destination: String,
        onProgress: (Float) -> Unit,
    ): Result<Unit>
}
