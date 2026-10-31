package com.emusync.runner

import kotlinx.coroutines.delay

internal data class ProcessLaunch(
    val name: String,
    val existingIds: Set<Long>,
    val request: Process?,
    val desktopHandoffSucceeded: Boolean,
)

internal data class ProcessPolling(
    val appearanceTimeoutMs: Long,
    val appearanceIntervalMs: Long,
    val exitIntervalMs: Long,
    val reuseConfirmationDelayMs: Long,
)

/** Tracks a new instance or a confirmed reuse without waiting for unrelated existing processes. */
internal class ExternalProcessTracker(
    private val matchingProcessIds: (String) -> Set<Long>,
    private val isSteamShaderCompiling: () -> Boolean,
    private val isProcessAlive: (Long) -> Boolean = { pid ->
        ProcessHandle.of(pid).map { it.isAlive }.orElse(false)
    },
    private val currentTimeMs: () -> Long = System::currentTimeMillis,
) {
    suspend fun waitForProcess(launch: ProcessLaunch, polling: ProcessPolling): Boolean {
        val tracked = awaitAppearance(launch, polling).toMutableSet()
        val appeared = tracked.isNotEmpty()
        awaitExit(launch, tracked, polling.exitIntervalMs)
        return appeared
    }

    private class AppearanceState(var startedAt: Long, var successfulRequestAt: Long? = null)

    private suspend fun awaitAppearance(launch: ProcessLaunch, polling: ProcessPolling): Set<Long> {
        val state = AppearanceState(currentTimeMs())
        // A non-null outcome ends the wait; an empty set means the launch request failed.
        var outcome: Set<Long>? = null
        while (outcome == null && currentTimeMs() - state.startedAt < polling.appearanceTimeoutMs) {
            outcome = observeAppearance(launch, state, polling.reuseConfirmationDelayMs)
            if (outcome == null) {
                if (isSteamShaderCompiling()) state.startedAt = currentTimeMs()
                delay(polling.appearanceIntervalMs)
            }
        }
        return outcome.orEmpty()
    }

    private fun observeAppearance(
        launch: ProcessLaunch,
        state: AppearanceState,
        confirmationDelayMs: Long,
    ): Set<Long>? {
        val newMatches = matchingProcessIds(launch.name) - launch.existingIds
        if (newMatches.isNotEmpty()) return newMatches

        val exitCode =
            if (launch.request?.isAlive == false) {
                runCatching { launch.request.exitValue() }.getOrNull()
            } else null
        return if (exitCode != null && exitCode != 0) emptySet()
        else confirmedReuse(launch, state, confirmationDelayMs, exitCode)
    }

    private fun confirmedReuse(
        launch: ProcessLaunch,
        state: AppearanceState,
        confirmationDelayMs: Long,
        exitCode: Int?,
    ): Set<Long>? {
        if (!launch.desktopHandoffSucceeded && exitCode != 0) {
            state.successfulRequestAt = null
            return null
        }
        val now = currentTimeMs()
        val confirmedAt = state.successfulRequestAt ?: now.also { state.successfulRequestAt = it }
        return if (now - confirmedAt < confirmationDelayMs) null
        else
            matchingProcessIds(launch.name).intersect(launch.existingIds).takeIf { it.isNotEmpty() }
    }

    private suspend fun awaitExit(
        launch: ProcessLaunch,
        tracked: MutableSet<Long>,
        intervalMs: Long,
    ) {
        while (tracked.isNotEmpty()) {
            val activeMatches =
                matchingProcessIds(launch.name).filterTo(mutableSetOf()) {
                    it !in launch.existingIds || it in tracked
                }
            tracked += activeMatches
            tracked.removeIf { !isProcessAlive(it) }
            if (tracked.isNotEmpty()) delay(intervalMs)
        }
    }
}
