package com.emusync.domain.execution

import com.emusync.domain.library.GameEntry

data class GameLaunchRequest(
    val entry: GameEntry,
    val romPath: String? = null,
    val steamAppId: Int? = null,
)
