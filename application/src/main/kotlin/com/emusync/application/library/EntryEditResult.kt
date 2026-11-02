package com.emusync.application.library

import com.emusync.domain.config.AppConfig
import com.emusync.domain.library.GameEntry

data class EntryEditResult(val config: AppConfig, val entry: GameEntry)
