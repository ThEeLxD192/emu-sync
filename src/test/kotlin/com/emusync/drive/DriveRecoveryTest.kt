package com.emusync.drive

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

class DriveRecoveryTest {
    @Test
    fun `recovery receives the original failure`() {
        val failure = IllegalStateException("transport failure")
        val result =
            recoverDriveFailure(
                action = { throw failure },
                recover = {
                    assertSame(failure, it)
                    "recovered"
                },
            )
        assertEquals("recovered", result)
    }

    @Test
    fun `cancellation escapes without invoking recovery`() {
        val cancellation = CancellationException("cancel transfer")
        var recovered = false
        val thrown =
            assertFailsWith<CancellationException> {
                recoverDriveFailure(
                    action = { throw cancellation },
                    recover = { recovered = true },
                )
            }
        assertSame(cancellation, thrown)
        assertFalse(recovered)
    }

    @Test
    fun `suspending actions retain the caller coroutine`() = runTest {
        val expectedJob = currentCoroutineContext()[Job]
        val result =
            recoverDriveFailure(
                action = {
                    delay(1)
                    assertSame(expectedJob, currentCoroutineContext()[Job])
                    "completed"
                },
                recover = { throw AssertionError("Unexpected recovery", it) },
            )
        assertEquals("completed", result)
    }
}
