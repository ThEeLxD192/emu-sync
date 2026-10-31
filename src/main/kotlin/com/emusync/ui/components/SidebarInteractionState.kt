package com.emusync.ui.components

import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import com.emusync.domain.library.GameEntry
import kotlin.math.roundToInt

private const val DRAGGED_ROW_Z_INDEX = 10f

/** Owns collapse and drag state independently of the composables that display each row. */
internal class SidebarInteractionState {
    var draggingKey by mutableStateOf<String?>(null)
        private set

    var dragOffsetY by mutableStateOf(0f)
        private set

    var collapsedGroups by mutableStateOf(emptySet<String>())
        private set

    fun startDrag(key: String) {
        draggingKey = key
        dragOffsetY = 0f
    }

    fun endDrag() {
        draggingKey = null
        dragOffsetY = 0f
    }

    fun toggleGroup(row: SidebarRow.FolderHeader) {
        if (row.isCollapsed) expandGroup(row.groupName)
        else collapsedGroups = collapsedGroups + row.groupName
    }

    fun activateFolder(row: SidebarRow.FolderHeader, onEntrySelected: (GameEntry) -> Unit) {
        if (row.isCollapsed) expandGroup(row.groupName)
        if (!row.hasSelectedChild) row.firstChild?.let(onEntrySelected)
        else if (!row.isCollapsed) collapsedGroups = collapsedGroups + row.groupName
    }

    fun rowModifier(key: String): Modifier =
        if (draggingKey == key) {
            Modifier.zIndex(DRAGGED_ROW_Z_INDEX)
                .offset { IntOffset(0, dragOffsetY.roundToInt()) }
                .shadow(8.dp, RoundedCornerShape(8.dp))
        } else Modifier.zIndex(1f)

    fun dragFolder(
        entries: List<GameEntry>,
        row: SidebarRow.FolderHeader,
        delta: Float,
        itemHeightPx: Float,
        onReorder: (List<GameEntry>) -> Unit,
    ) {
        applyDrag(row.key, delta, itemHeightPx, onReorder) { steps ->
            val units = partitionIntoUnits(entries)
            val current = units.indexOfFirst {
                it is SidebarUnit.GroupUnit && it.groupName.equals(row.groupName, ignoreCase = true)
            }
            clampedMove(current, units.lastIndex, steps) { target ->
                moveTopLevelUnit(entries, current, target)
            }
        }
    }

    fun dragEntry(
        entries: List<GameEntry>,
        row: SidebarRow.EntryRow,
        delta: Float,
        itemHeightPx: Float,
        onReorder: (List<GameEntry>) -> Unit,
    ) {
        applyDrag(row.key, delta, itemHeightPx, onReorder) { steps ->
            entryDragMove(entries, row, steps)
        }
    }

    private fun applyDrag(
        key: String,
        delta: Float,
        itemHeightPx: Float,
        onReorder: (List<GameEntry>) -> Unit,
        computeMove: (Int) -> SidebarDragMove?,
    ) {
        dragOffsetY += delta
        if (draggingKey != key) return
        val move = computeMove((dragOffsetY / itemHeightPx).toInt()) ?: return
        onReorder(move.entries)
        dragOffsetY -= move.steps * itemHeightPx
    }

    private fun expandGroup(name: String) {
        collapsedGroups = collapsedGroups.filterNot { it.equals(name, ignoreCase = true) }.toSet()
    }
}

private data class SidebarDragMove(val entries: List<GameEntry>, val steps: Int)

private fun entryDragMove(
    entries: List<GameEntry>,
    row: SidebarRow.EntryRow,
    steps: Int,
): SidebarDragMove? =
    if (row.isNested && row.groupName != null) nestedEntryDragMove(entries, row, steps)
    else {
        val units = partitionIntoUnits(entries)
        val current = units.indexOfFirst {
            it is SidebarUnit.StandaloneUnit && it.entry.name == row.entry.name
        }
        clampedMove(current, units.lastIndex, steps) { target ->
            moveTopLevelUnit(entries, current, target)
        }
    }

private fun nestedEntryDragMove(
    entries: List<GameEntry>,
    row: SidebarRow.EntryRow,
    steps: Int,
): SidebarDragMove? {
    val group =
        partitionIntoUnits(entries).firstOrNull {
            it is SidebarUnit.GroupUnit && it.groupName.equals(row.groupName, ignoreCase = true)
        } as? SidebarUnit.GroupUnit ?: return null
    val current = group.entries.indexOfFirst { it.name == row.entry.name }
    return clampedMove(current, group.entries.lastIndex, steps) { target ->
        moveChildEntry(entries, group.groupName, current, target)
    }
}

private fun clampedMove(
    current: Int,
    lastIndex: Int,
    steps: Int,
    reorder: (Int) -> List<GameEntry>,
): SidebarDragMove? {
    if (current == -1) return null
    val target = (current + steps).coerceIn(0, lastIndex)
    return if (target == current) null else SidebarDragMove(reorder(target), target - current)
}
