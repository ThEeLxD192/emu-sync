package com.emusync.domain.update

interface UpdateInstaller {
    fun isSteamGameMode(): Boolean

    fun restart(path: String): Boolean
}
