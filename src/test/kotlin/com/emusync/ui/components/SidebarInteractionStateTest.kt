package com.emusync.ui.components

import com.emusync.model.GameEntry
import com.emusync.model.NativePCGame
import kotlin.test.assertEquals
import kotlin.test.assertSame
import kotlin.test.assertTrue
import org.junit.jupiter.api.Test

class SidebarInteractionStateTest {
    private val first: GameEntry = NativePCGame("First", "/first", group = "Group")
    private val second: GameEntry = NativePCGame("Second", "/second", group = "Group")
    private val standalone: GameEntry = NativePCGame("Standalone", "/standalone")
    private val entries = listOf(first, second, standalone)

    @Test
    fun `activating a folder selects its first child then toggles collapse around that selection`() {
        val state = SidebarInteractionState()
        val selected = mutableListOf<GameEntry>()
        state.activateFolder(header(state), selected::add)
        assertEquals(listOf(first), selected)
        assertTrue(state.collapsedGroups.isEmpty())

        state.activateFolder(header(state, first), selected::add)
        assertEquals(setOf("Group"), state.collapsedGroups)
        state.activateFolder(header(state, first), selected::add)

        assertTrue(state.collapsedGroups.isEmpty())
        assertEquals(listOf(first), selected)
    }

    @Test
    fun `opening a collapsed unselected folder expands it and selects its first child`() {
        val state = SidebarInteractionState()
        state.toggleGroup(header(state))
        val selected = mutableListOf<GameEntry>()

        state.activateFolder(header(state), selected::add)

        assertTrue(state.collapsedGroups.isEmpty())
        assertEquals(listOf(first), selected)
    }

    @Test
    fun `expanding a folder removes collapsed names regardless of case`() {
        val state = SidebarInteractionState()
        state.toggleGroup(header(state))

        state.toggleGroup(header(state).copy(groupName = "gRoUp"))

        assertTrue(state.collapsedGroups.isEmpty())
    }

    @Test
    fun `folder dragging accumulates partial movement and moves the whole group`() {
        val state = SidebarInteractionState()
        val row = header(state)
        val reorders = mutableListOf<List<GameEntry>>()
        state.startDrag(row.key)
        state.dragFolder(entries, row, 25f, 52f, reorders::add)
        assertTrue(reorders.isEmpty())

        state.dragFolder(entries, row, 35f, 52f, reorders::add)

        assertEquals(listOf(listOf(standalone, first, second)), reorders)
        assertEquals(8f, state.dragOffsetY)
        state.endDrag()
        assertEquals(null, state.draggingKey)
        assertEquals(0f, state.dragOffsetY)
    }

    @Test
    fun `child dragging stays within its group even beyond the last sibling`() {
        val state = SidebarInteractionState()
        val row = entryRow(first)
        val reorders = mutableListOf<List<GameEntry>>()
        state.startDrag(row.key)

        state.dragEntry(entries, row, 1_000f, 52f, reorders::add)

        assertEquals(listOf(listOf(second, first, standalone)), reorders)
        assertEquals(948f, state.dragOffsetY)
        assertEquals(listOf(first, second, standalone), entries)
    }

    @Test
    fun `standalone dragging crosses a whole group without splitting its children`() {
        val state = SidebarInteractionState()
        val row = entryRow(standalone)
        val reorders = mutableListOf<List<GameEntry>>()
        state.startDrag(row.key)

        state.dragEntry(entries, row, -52f, 52f, reorders::add)

        assertEquals(listOf(listOf(standalone, first, second)), reorders)
        assertEquals(0f, state.dragOffsetY)
    }

    @Test
    fun `an entry removed during dragging cannot reorder the remaining library`() {
        val state = SidebarInteractionState()
        val row = entryRow(first)
        state.startDrag(row.key)

        state.dragEntry(listOf(second, standalone), row, 52f, 52f) {
            error("A removed entry must not reorder the library")
        }

        assertEquals(52f, state.dragOffsetY)
    }

    @Test
    fun `invalid sibling moves keep the original order and list instance`() {
        assertSame(entries, moveChildEntry(entries, "Missing", 0, 1))
        assertSame(entries, moveChildEntry(entries, "Group", -1, 1))
        assertSame(entries, moveChildEntry(entries, "Group", 0, 2))
        assertSame(entries, moveChildEntry(entries, "Group", 0, 0))
    }

    private fun header(state: SidebarInteractionState, selected: GameEntry? = null) =
        computeSidebarRows(entries, state.collapsedGroups, selected).first()
            as SidebarRow.FolderHeader

    private fun entryRow(entry: GameEntry) =
        computeSidebarRows(entries, emptySet(), null)
            .filterIsInstance<SidebarRow.EntryRow>()
            .first { it.entry == entry }
}
