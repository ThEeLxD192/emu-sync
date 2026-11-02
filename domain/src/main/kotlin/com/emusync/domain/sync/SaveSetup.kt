package com.emusync.domain.sync

/** Supplies save destinations when needed; an empty target list cancels a cloud restore. */
fun interface SaveSetup {
    suspend fun request(request: SaveSetupRequest): SaveSetupResult
}
