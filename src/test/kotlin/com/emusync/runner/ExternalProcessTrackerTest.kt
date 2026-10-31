package com.emusync.runner

import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ExternalProcessTrackerTest {
    private val polling = ProcessPolling(10, 2, 2, 4)

    @Test
    fun `appearance timeout ends without tracking unrelated existing processes`() = runTest {
        var livenessChecks = 0
        val tracker =
            ExternalProcessTracker(
                matchingProcessIds = { setOf(99L) },
                isSteamShaderCompiling = { false },
                isProcessAlive = {
                    livenessChecks++
                    true
                },
                currentTimeMs = { testScheduler.currentTime },
            )

        assertFalse(tracker.waitForProcess(ProcessLaunch("game", setOf(99L), null, false), polling))
        assertEquals(10L, testScheduler.currentTime)
        assertEquals(0, livenessChecks)
    }

    @Test
    fun `shader compilation renews the timeout until the game appears and exits`() = runTest {
        val tracker =
            ExternalProcessTracker(
                matchingProcessIds = {
                    if (testScheduler.currentTime in 25L until 30L) setOf(1L) else emptySet()
                },
                isSteamShaderCompiling = { testScheduler.currentTime < 20 },
                isProcessAlive = { testScheduler.currentTime < 30 },
                currentTimeMs = { testScheduler.currentTime },
            )

        assertTrue(tracker.waitForProcess(ProcessLaunch("game", emptySet(), null, false), polling))
        assertEquals(30L, testScheduler.currentTime)
    }

    @Test
    fun `timeout resumes after shader compilation stops without a game appearing`() = runTest {
        val tracker =
            ExternalProcessTracker(
                matchingProcessIds = { emptySet() },
                isSteamShaderCompiling = { testScheduler.currentTime < 20 },
                currentTimeMs = { testScheduler.currentTime },
            )

        assertFalse(tracker.waitForProcess(ProcessLaunch("game", emptySet(), null, false), polling))
        assertEquals(28L, testScheduler.currentTime)
    }

    @Test
    fun `desktop handoff confirms reuse and waits for the existing game to exit`() = runTest {
        val tracker =
            ExternalProcessTracker(
                matchingProcessIds = {
                    if (testScheduler.currentTime < 8) setOf(1L) else emptySet()
                },
                isSteamShaderCompiling = { false },
                isProcessAlive = { testScheduler.currentTime < 8 },
                currentTimeMs = { testScheduler.currentTime },
            )

        assertTrue(tracker.waitForProcess(ProcessLaunch("game", setOf(1L), null, true), polling))
        assertEquals(8L, testScheduler.currentTime)
    }

    @Test
    fun `tracking follows replacement processes and excludes a preexisting decoy`() = runTest {
        val tracker =
            ExternalProcessTracker(
                matchingProcessIds = {
                    setOf(99L) +
                        when {
                            testScheduler.currentTime < 2 -> setOf(1L)
                            testScheduler.currentTime < 6 -> setOf(2L)
                            else -> emptySet()
                        }
                },
                isSteamShaderCompiling = { false },
                isProcessAlive = { pid ->
                    when (pid) {
                        1L -> testScheduler.currentTime < 2
                        2L -> testScheduler.currentTime < 6
                        else -> true
                    }
                },
                currentTimeMs = { testScheduler.currentTime },
            )

        assertTrue(tracker.waitForProcess(ProcessLaunch("game", setOf(99L), null, false), polling))
        assertEquals(6L, testScheduler.currentTime)
    }

    @Test
    fun `caller cancellation interrupts waiting for a running game`() = runTest {
        val tracker =
            ExternalProcessTracker(
                matchingProcessIds = { setOf(1L) },
                isSteamShaderCompiling = { false },
                isProcessAlive = { true },
                currentTimeMs = { testScheduler.currentTime },
            )
        val tracking = async {
            tracker.waitForProcess(ProcessLaunch("game", emptySet(), null, false), polling)
        }
        runCurrent()
        assertFalse(tracking.isCompleted)

        tracking.cancelAndJoin()

        assertTrue(tracking.isCancelled)
        assertFailsWith<CancellationException> { tracking.await() }
    }
}
