package com.emusync.runner

internal data class ProcessPolling(
    val appearanceTimeoutMs: Long,
    val appearanceIntervalMs: Long,
    val exitIntervalMs: Long,
    val reuseConfirmationDelayMs: Long,
)
