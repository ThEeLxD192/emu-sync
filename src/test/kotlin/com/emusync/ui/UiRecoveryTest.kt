package com.emusync.ui

import java.io.IOException
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertSame
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test

class UiRecoveryTest {
    @Test
    fun `an error boundary receives the original failure and returns its recovery result`() {
        val failure = IOException("Storage unavailable")

        val result =
            recoverUiFailure(
                action = { throw failure },
                recover = {
                    assertSame(failure, it)
                    "reported"
                },
            )

        assertEquals("reported", result)
    }

    @Test
    fun `cancellation bypasses normal error reporting`() {
        val cancellation = CancellationException("Cancelled UI action")
        var recovered = false

        val thrown =
            assertFailsWith<CancellationException> {
                recoverUiFailure(action = { throw cancellation }, recover = { recovered = true })
            }

        assertSame(cancellation, thrown)
        assertFalse(recovered)
    }

    @Test
    fun `cancelled suspending actions restore transient state before the caller resumes`() =
        runTest {
            val events = mutableListOf<String>()
            val expectedJob = currentCoroutineContext()[Job]

            assertFailsWith<CancellationException> {
                restoreUiOnCancellation(
                    action = {
                        events += "started"
                        delay(1)
                        assertSame(expectedJob, currentCoroutineContext()[Job])
                        throw CancellationException("Cancelled while waiting")
                    },
                    restore = { events += "restored" },
                )
            }
            events += "caller"

            assertEquals(listOf("started", "restored", "caller"), events)
        }

    @Test
    fun `successful actions retain their result and do not restore cancellation state`() {
        var restored = false

        val result = restoreUiOnCancellation(action = { "ready" }, restore = { restored = true })

        assertEquals("ready", result)
        assertFalse(restored)
    }

    @Test
    fun `ordinary failures are propagated without applying cancellation cleanup`() {
        val failure = IllegalStateException("Failed operation")
        var restored = false

        val thrown =
            assertFailsWith<IllegalStateException> {
                restoreUiOnCancellation(action = { throw failure }, restore = { restored = true })
            }

        assertSame(failure, thrown)
        assertFalse(restored)
    }
}
