package com.emusync.model

import org.junit.jupiter.api.Test
import kotlin.test.assertEquals

class GameTitleUtilsTest {

    @Test
    fun `cleanGameTitle replaces underscores with spaces`() {
        assertEquals("Super Mario World", cleanGameTitle("Super_Mario_World"))
        assertEquals("The Legend of Zelda", cleanGameTitle("The_Legend_of_Zelda"))
    }

    @Test
    fun `cleanGameTitle removes dump tags in brackets`() {
        assertEquals("Super Mario World", cleanGameTitle("Super_Mario_World_[U]_[!]"))
        assertEquals("Chrono Trigger", cleanGameTitle("Chrono_Trigger_[T+Spa]"))
        assertEquals("Pokemon FireRed", cleanGameTitle("Pokemon_FireRed_[v1.1]_[b1]"))
    }

    @Test
    fun `cleanGameTitle removes region and language tags in parentheses`() {
        assertEquals("Pokemon - Emerald Version", cleanGameTitle("Pokemon - Emerald Version (USA, Europe)"))
        assertEquals("The Legend of Zelda", cleanGameTitle("The_Legend_of_Zelda_(USA)_(En,Fr,De)"))
        assertEquals("Sonic The Hedgehog 2", cleanGameTitle("Sonic_The_Hedgehog_2_(W)_(REV01)"))
    }

    @Test
    fun `cleanGameTitle preserves multi-disc and multi-part indicators in parentheses`() {
        assertEquals("Crash Bandicoot (Disc 1)", cleanGameTitle("Crash_Bandicoot_(USA)_(Disc 1)"))
        assertEquals("Final Fantasy VII (Disc 2)", cleanGameTitle("Final Fantasy VII (USA) (Disc 2)"))
        assertEquals("Metal Gear Solid (CD 1)", cleanGameTitle("Metal_Gear_Solid_(Europe)_(CD 1)"))
        assertEquals("Shenmue (Part 3)", cleanGameTitle("Shenmue (USA) (Part 3)"))
    }

    @Test
    fun `cleanGameTitle strips file extension if present`() {
        assertEquals("Super Mario 64", cleanGameTitle("Super_Mario_64_(USA).z64"))
        assertEquals("Mario Kart DS", cleanGameTitle("Mario_Kart_DS_(USA).nds"))
    }

    @Test
    fun `cleanGameTitle collapses extra whitespace and trims`() {
        assertEquals("Game Name Here", cleanGameTitle("  Game   Name    Here   "))
    }

    @Test
    fun `cleanGameTitle falls back to original trimmed if everything was stripped`() {
        assertEquals("[!]", cleanGameTitle("[!]"))
        assertEquals("(USA)", cleanGameTitle("(USA)"))
    }

    @Test
    fun `normalizeGameKey matches games across different naming conventions`() {
        val pc1Name = "Super_Mario_World_[U]_[!].sfc"
        val pc2Name = "Super Mario World (USA).sfc"
        val cleanName = "Super Mario World"

        val key1 = normalizeGameKey(pc1Name)
        val key2 = normalizeGameKey(pc2Name)
        val key3 = normalizeGameKey(cleanName)

        assertEquals("super mario world", key1)
        assertEquals(key1, key2)
        assertEquals(key1, key3)
    }

    @Test
    fun `EmulatorSystem getEffectiveTitle resolves exact match first`() {
        val emu = EmulatorSystem(
            name = "GBA",
            executablePath = "/bin/mgba",
            romsDirectory = "/roms/gba",
            extensions = listOf("gba"),
            titleByRom = mapOf("zelda.gba" to "Zelda: Minish Cap"),
        )
        assertEquals("Zelda: Minish Cap", emu.getEffectiveTitle("zelda.gba"))
    }

    @Test
    fun `EmulatorSystem getEffectiveTitle resolves match without extension`() {
        val emu = EmulatorSystem(
            name = "GBA",
            executablePath = "/bin/mgba",
            romsDirectory = "/roms/gba",
            extensions = listOf("gba"),
            titleByRom = mapOf("zelda" to "Zelda: Minish Cap"),
        )
        assertEquals("Zelda: Minish Cap", emu.getEffectiveTitle("zelda.gba"))
    }

    @Test
    fun `EmulatorSystem getEffectiveTitle resolves flexible normalized cross-device match`() {
        // PC 1 saved title under "Super_Mario_World_[U].sfc"
        val emu = EmulatorSystem(
            name = "SNES",
            executablePath = "/bin/snes9x",
            romsDirectory = "/roms/snes",
            extensions = listOf("sfc"),
            titleByRom = mapOf("Super_Mario_World_[U].sfc" to "Super Mario World - Custom Title"),
        )

        // PC 2 scans a ROM named "Super Mario World (USA).sfc"
        assertEquals("Super Mario World - Custom Title", emu.getEffectiveTitle("Super Mario World (USA).sfc"))
    }

    @Test
    fun `EmulatorSystem getEffectiveTitle falls back to cleanGameTitle when no override set`() {
        val emu = EmulatorSystem(
            name = "SNES",
            executablePath = "/bin/snes9x",
            romsDirectory = "/roms/snes",
            extensions = listOf("sfc"),
            titleByRom = emptyMap(),
        )
        assertEquals("Super Mario World", emu.getEffectiveTitle("Super_Mario_World_[U]_[!].sfc", fallback = "Super_Mario_World_[U]_[!]"))
    }
}
