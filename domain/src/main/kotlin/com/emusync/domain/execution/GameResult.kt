package com.emusync.domain.execution

/** Exit code and elapsed duration of a completed game session. */
data class GameResult(val exitCode: Int, val durationMs: Long)
