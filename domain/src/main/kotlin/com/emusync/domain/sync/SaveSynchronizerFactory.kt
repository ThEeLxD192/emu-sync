package com.emusync.domain.sync

import com.emusync.domain.config.AppConfig

fun interface SaveSynchronizerFactory {
    fun create(config: AppConfig, onStatus: (SyncEvent) -> Unit): SaveSynchronizer
}
