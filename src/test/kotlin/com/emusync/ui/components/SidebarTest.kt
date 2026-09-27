package com.emusync.ui.components

import com.emusync.model.EmulatorSystem
import com.emusync.model.NativePCGame
import org.junit.jupiter.api.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class SidebarTest {

    private val eden = EmulatorSystem(
        name = "Eden",
        executablePath = "/usr/bin/eden",
        romsDirectory = "/roms/switch",
        extensions = listOf("nsp", "xci"),
        group = "Switch",
    )

    private val yuzu = EmulatorSystem(
        name = "Yuzu",
        executablePath = "/usr/bin/yuzu",
        romsDirectory = "/roms/switch",
        extensions = listOf("nsp", "xci"),
        group = "Switch",
    )

    private val pcsx2 = EmulatorSystem(
        name = "PCSX2",
        executablePath = "/usr/bin/pcsx2",
        romsDirectory = "/roms/ps2",
        extensions = listOf("iso"),
        group = null,
    )

    private val hollowKnight = NativePCGame(
        name = "Hollow Knight",
        executablePath = "/games/hk/start.sh",
        group = null,
    )

    @Test
    fun `computeSidebarRows includes FolderHeader and child items when expanded`() {
        val entries = listOf(eden, pcsx2, yuzu)
        val rows = computeSidebarRows(
            entries = entries,
            collapsedGroups = emptySet(),
            selectedEntry = null,
        )

        // Expected: FolderHeader(Switch), EntryRow(Eden, isNested=true), EntryRow(Yuzu, isNested=true), EntryRow(PCSX2, isNested=false)
        assertEquals(4, rows.size)

        val header = rows[0] as SidebarRow.FolderHeader
        assertEquals("Switch", header.groupName)
        assertEquals(2, header.count)
        assertFalse(header.isCollapsed)
        assertFalse(header.hasSelectedChild)
        assertEquals(eden, header.firstChild)
        assertEquals(0, header.unitIndex)
        assertEquals(2, header.totalUnits) // 2 units: Switch folder and PCSX2

        val child1 = rows[1] as SidebarRow.EntryRow
        assertEquals(eden, child1.entry)
        assertTrue(child1.isNested)
        assertEquals(0, child1.siblingIndex)
        assertEquals(2, child1.totalSiblings)

        val child2 = rows[2] as SidebarRow.EntryRow
        assertEquals(yuzu, child2.entry)
        assertTrue(child2.isNested)
        assertEquals(1, child2.siblingIndex)
        assertEquals(2, child2.totalSiblings)

        val standalone = rows[3] as SidebarRow.EntryRow
        assertEquals(pcsx2, standalone.entry)
        assertFalse(standalone.isNested)
        assertEquals(1, standalone.unitIndex)
    }

    @Test
    fun `computeSidebarRows keeps FolderHeader and excludes children when collapsed`() {
        val entries = listOf(eden, yuzu, pcsx2)
        val rows = computeSidebarRows(
            entries = entries,
            collapsedGroups = setOf("Switch"),
            selectedEntry = null,
        )

        // When collapsed, FolderHeader is NEVER removed! It stays in rows, but children are excluded.
        assertEquals(2, rows.size)

        val header = rows[0] as SidebarRow.FolderHeader
        assertEquals("Switch", header.groupName)
        assertEquals(2, header.count)
        assertTrue(header.isCollapsed)

        val standalone = rows[1] as SidebarRow.EntryRow
        assertEquals(pcsx2, standalone.entry)
        assertFalse(standalone.isNested)
    }

    @Test
    fun `FolderHeader is never removed when toggling collapsedGroups back and forth`() {
        val entries = listOf(eden, yuzu)

        // 1. Initially expanded
        val expandedRows = computeSidebarRows(entries, emptySet(), null)
        assertEquals(3, expandedRows.size)
        assertTrue(expandedRows[0] is SidebarRow.FolderHeader)
        assertEquals("Switch", (expandedRows[0] as SidebarRow.FolderHeader).groupName)

        // 2. Collapsed
        val collapsedRows = computeSidebarRows(entries, setOf("Switch"), null)
        assertEquals(1, collapsedRows.size)
        assertTrue(collapsedRows[0] is SidebarRow.FolderHeader)
        assertEquals("Switch", (collapsedRows[0] as SidebarRow.FolderHeader).groupName)

        // 3. Re-expanded
        val reExpandedRows = computeSidebarRows(entries, emptySet(), null)
        assertEquals(3, reExpandedRows.size)
        assertTrue(reExpandedRows[0] is SidebarRow.FolderHeader)
        assertEquals("Switch", (reExpandedRows[0] as SidebarRow.FolderHeader).groupName)
    }

    @Test
    fun `computeSidebarRows groups case-insensitively and respects case-insensitive collapse`() {
        val yuzuLower = yuzu.copy(group = "switch")
        val entries = listOf(eden, yuzuLower)

        // Both 'Switch' and 'switch' should be grouped under a single header
        val rows = computeSidebarRows(entries, emptySet(), null)
        assertEquals(3, rows.size)
        val header = rows[0] as SidebarRow.FolderHeader
        assertEquals(2, header.count)

        // Collapsing with lowercase 'switch' collapses 'Switch'
        val collapsedWithLower = computeSidebarRows(entries, setOf("switch"), null)
        assertEquals(1, collapsedWithLower.size)
        assertTrue((collapsedWithLower[0] as SidebarRow.FolderHeader).isCollapsed)

        // Collapsing with uppercase 'SWITCH' collapses 'Switch'
        val collapsedWithUpper = computeSidebarRows(entries, setOf("SWITCH"), null)
        assertEquals(1, collapsedWithUpper.size)
        assertTrue((collapsedWithUpper[0] as SidebarRow.FolderHeader).isCollapsed)
    }

    @Test
    fun `computeSidebarRows sets hasSelectedChild when child is selected`() {
        val entries = listOf(eden, yuzu, pcsx2)

        val rowsWithEdenSelected = computeSidebarRows(entries, emptySet(), eden)
        val header1 = rowsWithEdenSelected[0] as SidebarRow.FolderHeader
        assertTrue(header1.hasSelectedChild)

        val rowsWithPcsx2Selected = computeSidebarRows(entries, emptySet(), pcsx2)
        val header2 = rowsWithPcsx2Selected[0] as SidebarRow.FolderHeader
        assertFalse(header2.hasSelectedChild)
    }

    @Test
    fun `computeSidebarRows handles mixed groups and standalone entries correctly`() {
        val citra = EmulatorSystem(
            name = "Citra",
            executablePath = "/usr/bin/citra",
            romsDirectory = "/roms/3ds",
            extensions = listOf("3ds"),
            group = "3DS",
        )
        val entries = listOf(hollowKnight, eden, pcsx2, yuzu, citra)

        val rows = computeSidebarRows(entries, setOf("3DS"), null)

        assertEquals(6, rows.size)

        val row0 = rows[0] as SidebarRow.EntryRow
        assertEquals("Hollow Knight", row0.entry.name)
        assertFalse(row0.isNested)

        val row1 = rows[1] as SidebarRow.FolderHeader
        assertEquals("Switch", row1.groupName)
        assertEquals(2, row1.count)
        assertFalse(row1.isCollapsed)

        val row2 = rows[2] as SidebarRow.EntryRow
        assertEquals("Eden", row2.entry.name)
        assertTrue(row2.isNested)

        val row3 = rows[3] as SidebarRow.EntryRow
        assertEquals("Yuzu", row3.entry.name)
        assertTrue(row3.isNested)

        val row4 = rows[4] as SidebarRow.EntryRow
        assertEquals("PCSX2", row4.entry.name)
        assertFalse(row4.isNested)

        val row5 = rows[5] as SidebarRow.FolderHeader
        assertEquals("3DS", row5.groupName)
        assertEquals(1, row5.count)
        assertTrue(row5.isCollapsed)
    }

    @Test
    fun `partitionIntoUnits and flattenUnits preserves contiguous groups and entries`() {
        val entries = listOf(eden, pcsx2, yuzu)
        val units = partitionIntoUnits(entries)

        assertEquals(2, units.size)
        assertTrue(units[0] is SidebarUnit.GroupUnit)
        assertEquals("Switch", (units[0] as SidebarUnit.GroupUnit).groupName)
        assertEquals(listOf(eden, yuzu), (units[0] as SidebarUnit.GroupUnit).entries)

        assertTrue(units[1] is SidebarUnit.StandaloneUnit)
        assertEquals(pcsx2, (units[1] as SidebarUnit.StandaloneUnit).entry)

        val flattened = flattenUnits(units)
        assertEquals(listOf(eden, yuzu, pcsx2), flattened)
    }

    @Test
    fun `moveTopLevelUnit allows dragging folder past standalone entries and other folders`() {
        val citra = EmulatorSystem(
            name = "Citra",
            executablePath = "/usr/bin/citra",
            romsDirectory = "/roms/3ds",
            extensions = listOf("3ds"),
            group = "3DS",
        )
        // Order: [Switch (Eden, Yuzu), PCSX2, 3DS (Citra)]
        val entries = listOf(eden, yuzu, pcsx2, citra)

        // Move Unit 0 (Switch) down to Unit 1 (after PCSX2)
        val afterMoveDown = moveTopLevelUnit(entries, 0, 1)
        assertEquals(listOf(pcsx2, eden, yuzu, citra), afterMoveDown)

        // Move Switch down to Unit 2 (to the end)
        val atEnd = moveTopLevelUnit(afterMoveDown, 1, 2)
        assertEquals(listOf(pcsx2, citra, eden, yuzu), atEnd)

        // Move Switch back up to Unit 0
        val backToTop = moveTopLevelUnit(atEnd, 2, 0)
        assertEquals(listOf(eden, yuzu, pcsx2, citra), backToTop)
    }

    @Test
    fun `moveChildEntry allows dragging children within their folder`() {
        val ryujinx = EmulatorSystem(
            name = "Ryujinx",
            executablePath = "/usr/bin/ryujinx",
            romsDirectory = "/roms/switch",
            extensions = listOf("nsp"),
            group = "Switch",
        )
        // Switch has: [Eden, Yuzu, Ryujinx]
        val entries = listOf(eden, yuzu, ryujinx, pcsx2)

        // Move Eden from index 0 to index 1 (between Yuzu and Ryujinx)
        val movedEden = moveChildEntry(entries, "Switch", 0, 1)
        assertEquals(listOf(yuzu, eden, ryujinx, pcsx2), movedEden)

        // Move Ryujinx from index 2 to index 0 (to the top of Switch)
        val ryujinxTop = moveChildEntry(movedEden, "Switch", 2, 0)
        assertEquals(listOf(ryujinx, yuzu, eden, pcsx2), ryujinxTop)
    }
}
