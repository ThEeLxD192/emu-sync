package com.emusync.application.library

import com.emusync.domain.config.AppConfig
import com.emusync.domain.library.GameEntry

class SyncCheckRequest
internal constructor(
    val entry: GameEntry,
    internal val config: AppConfig,
    internal val generation: Long,
    internal val operationRevision: Long,
)
