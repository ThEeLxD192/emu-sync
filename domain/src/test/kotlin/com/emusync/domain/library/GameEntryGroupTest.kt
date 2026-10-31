package com.emusync.domain.library

import kotlin.test.assertEquals
import kotlin.test.assertNull
import org.junit.jupiter.api.Test

class GameEntryGroupTest {
    @Test
    fun `effectiveCloudFolder defaults to name when group and cloudFolder are null`() {
        val emu =
            EmulatorSystem(
                name = "Yuzu",
                executablePath = "/usr/bin/yuzu",
                romsDirectory = "/roms/switch",
                extensions = listOf("nsp", "xci"),
            )
        assertEquals("Yuzu", emu.effectiveCloudFolder)
        assertNull(emu.group)
        assertNull(emu.cloudFolder)
    }

    @Test
    fun `effectiveCloudFolder uses group when group is set and cloudFolder is null`() {
        val eden =
            EmulatorSystem(
                name = "Eden",
                executablePath = "/usr/bin/eden",
                romsDirectory = "/roms/switch",
                extensions = listOf("nsp", "xci"),
                group = "Switch",
            )
        assertEquals("Switch", eden.effectiveCloudFolder)
        assertEquals("Switch", eden.group)
        assertNull(eden.cloudFolder)
    }

    @Test
    fun `effectiveCloudFolder uses cloudFolder over group when both are set`() {
        val yuzu =
            EmulatorSystem(
                name = "Yuzu Early Access",
                executablePath = "/usr/bin/yuzu-ea",
                romsDirectory = "/roms/switch",
                extensions = listOf("nsp", "xci"),
                group = "Switch (Experimental)",
                cloudFolder = "Switch",
            )
        assertEquals("Switch", yuzu.effectiveCloudFolder)
    }

    @Test
    fun `effectiveCloudFolder trims whitespace and falls back appropriately`() {
        val emu =
            EmulatorSystem(
                name = "Citra",
                executablePath = "/usr/bin/citra",
                romsDirectory = "/roms/3ds",
                extensions = listOf("3ds"),
                group = "   ",
                cloudFolder = "   ",
            )
        assertEquals("Citra", emu.effectiveCloudFolder)

        val emuWithPaddedGroup = emu.copy(group = "  Nintendo 3DS  ", cloudFolder = "   ")
        assertEquals("Nintendo 3DS", emuWithPaddedGroup.effectiveCloudFolder)
    }

    @Test
    fun `multiple emulators with same group share the exact same effectiveCloudFolder`() {
        val yuzu =
            EmulatorSystem(
                name = "Yuzu",
                executablePath = "/bin/yuzu",
                romsDirectory = "/roms/switch",
                extensions = listOf("nsp"),
                group = "Switch",
            )
        val eden =
            EmulatorSystem(
                name = "Eden",
                executablePath = "/bin/eden",
                romsDirectory = "/roms/switch",
                extensions = listOf("nsp"),
                group = "Switch",
            )
        val ryujinx =
            EmulatorSystem(
                name = "Ryujinx",
                executablePath = "/bin/ryujinx",
                romsDirectory = "/roms/switch",
                extensions = listOf("nsp"),
                group = "Switch",
            )

        assertEquals("Switch", yuzu.effectiveCloudFolder)
        assertEquals("Switch", eden.effectiveCloudFolder)
        assertEquals("Switch", ryujinx.effectiveCloudFolder)
        assertEquals(yuzu.effectiveCloudFolder, eden.effectiveCloudFolder)
        assertEquals(eden.effectiveCloudFolder, ryujinx.effectiveCloudFolder)
    }
}
