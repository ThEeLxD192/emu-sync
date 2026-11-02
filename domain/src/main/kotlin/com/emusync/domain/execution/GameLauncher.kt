package com.emusync.domain.execution

interface GameLauncher {
    suspend fun launch(request: GameLaunchRequest): GameResult
}
