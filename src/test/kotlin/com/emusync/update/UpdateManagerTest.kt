package com.emusync.update

import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test

class UpdateManagerTest {

    @Test
    fun `correctly compares semantic versions`() {
        assertTrue(UpdateManager.isNewerVersion("0.1.0", "0.1.1"))
        assertTrue(UpdateManager.isNewerVersion("0.1.0", "0.2.0"))
        assertTrue(UpdateManager.isNewerVersion("0.9.9", "1.0.0"))
        assertTrue(UpdateManager.isNewerVersion("0.1.0", "0.1.0.1"))

        assertFalse(UpdateManager.isNewerVersion("0.1.0", "0.1.0"))
        assertFalse(UpdateManager.isNewerVersion("0.1.1", "0.1.0"))
        assertFalse(UpdateManager.isNewerVersion("1.0.0", "0.9.9"))
        assertFalse(UpdateManager.isNewerVersion("0.2.0", "0.1.9"))
    }
}
