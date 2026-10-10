package com.emusync.util

import org.junit.jupiter.api.Test
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertFalse

class AppImageEnvironmentTest {
    @Test
    fun `removes appimage paths and variables while preserving other library paths`() {
        val environment = mutableMapOf(
            "APPDIR" to "/tmp/.mount_EmuSync123",
            "ARGV0" to "EmuSync",
            "OWD" to "/home/user",
            "LD_LIBRARY_PATH" to listOf(
                "/tmp/.mount_EmuSync123/lib/app",
                "/usr/lib",
                "/tmp/.mount_OtherApp456/usr/lib",
                "/opt/emulator/lib",
            ).joinToString(File.pathSeparator),
        )

        AppImageEnvironment.clean(environment)

        assertEquals(
            listOf("/usr/lib", "/opt/emulator/lib").joinToString(File.pathSeparator),
            environment["LD_LIBRARY_PATH"],
        )
        assertFalse(environment.containsKey("APPDIR"))
        assertFalse(environment.containsKey("ARGV0"))
        assertFalse(environment.containsKey("OWD"))
    }

    @Test
    fun `removes library path when it contains only appimage runtime paths`() {
        val environment = mutableMapOf(
            "LD_LIBRARY_PATH" to "/tmp/.mount_EmuSync123/lib/app",
        )

        AppImageEnvironment.clean(environment)

        assertFalse(environment.containsKey("LD_LIBRARY_PATH"))
    }
}
