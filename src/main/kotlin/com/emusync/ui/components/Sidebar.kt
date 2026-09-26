package com.emusync.ui.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.DragIndicator
import androidx.compose.material.icons.filled.FolderOpen
import androidx.compose.material.icons.filled.Gamepad
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.SportsEsports
import androidx.compose.material.icons.filled.Widgets
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.PointerIcon
import androidx.compose.ui.input.pointer.pointerHoverIcon
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.zIndex
import com.emusync.model.EmulatorSystem
import com.emusync.model.GameEntry
import com.emusync.model.NativePCGame
import com.emusync.ui.theme.EmuSyncColors
import java.awt.Cursor
import kotlin.math.roundToInt

/**
 * Sidebar showing the list of categories (emulator systems and native games).
 * Supports drag-and-drop reordering as well as quick up/down controls.
 */
@Composable
fun Sidebar(
    entries: List<GameEntry>,
    selectedEntry: GameEntry?,
    onEntrySelected: (GameEntry) -> Unit,
    onAddClicked: () -> Unit,
    onReorder: ((fromIndex: Int, toIndex: Int) -> Unit)? = null,
    onReorderEntries: ((List<GameEntry>) -> Unit)? = null,
    steamAvailable: Boolean = false,
    isSteamRegistered: (GameEntry) -> Boolean = { it.steamAppId != null },
    onSteamToggle: ((GameEntry) -> Unit)? = null,
    modifier: Modifier = Modifier,
) {
    var draggingKey by remember { mutableStateOf<String?>(null) }
    var dragOffsetY by remember { mutableStateOf(0f) }
    var collapsedGroups by remember { mutableStateOf(setOf<String>()) }
    val density = LocalDensity.current
    val itemHeightPx = with(density) { 52.dp.toPx() }

    val sidebarRows = remember(entries, collapsedGroups, selectedEntry) {
        computeSidebarRows(entries, collapsedGroups, selectedEntry)
    }

    val applyReorder: (List<GameEntry>) -> Unit = { newEntries ->
        if (onReorderEntries != null) {
            onReorderEntries(newEntries)
        } else if (onReorder != null) {
            // Find single moved entry if using legacy onReorder
            val fromIdx = entries.indices.firstOrNull { entries[it] != newEntries[it] } ?: 0
            val movedEntry = entries[fromIdx]
            val toIdx = newEntries.indexOf(movedEntry).coerceAtLeast(0)
            onReorder(fromIdx, toIdx)
        }
    }

    Column(modifier = modifier.padding(vertical = 4.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = "LIBRARY",
                style = MaterialTheme.typography.labelSmall,
                color = EmuSyncColors.OnSurfaceDim,
                letterSpacing = 1.5.sp,
                modifier = Modifier.weight(1f).padding(start = 4.dp),
            )
            IconButton(
                onClick = onAddClicked,
                modifier = Modifier.size(28.dp),
            ) {
                Icon(
                    Icons.Default.Add,
                    contentDescription = "Add entry",
                    tint = EmuSyncColors.OnSurfaceDim,
                    modifier = Modifier.size(18.dp),
                )
            }
        }

        LazyColumn(
            contentPadding = PaddingValues(horizontal = 8.dp),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            items(
                items = sidebarRows,
                key = { row ->
                    when (row) {
                        is SidebarRow.FolderHeader -> "folder_${row.groupName.lowercase()}"
                        is SidebarRow.EntryRow -> "entry_${row.entry.name}"
                    }
                }
            ) { row ->
                when (row) {
                    is SidebarRow.FolderHeader -> {
                        val folderKey = "folder_${row.groupName.lowercase()}"
                        val isDragging = draggingKey == folderKey
                        val itemModifier = if (isDragging) {
                            Modifier
                                .zIndex(10f)
                                .offset { IntOffset(0, dragOffsetY.roundToInt()) }
                                .shadow(8.dp, RoundedCornerShape(8.dp))
                        } else {
                            Modifier.zIndex(1f)
                        }

                        SidebarFolderHeader(
                            name = row.groupName,
                            count = row.count,
                            isCollapsed = row.isCollapsed,
                            hasSelectedChild = row.hasSelectedChild,
                            isDragging = isDragging,
                            canMoveUp = row.unitIndex > 0,
                            canMoveDown = row.unitIndex < row.totalUnits - 1,
                            onMoveUp = {
                                val newOrder = moveTopLevelUnit(entries, row.unitIndex, row.unitIndex - 1)
                                applyReorder(newOrder)
                            },
                            onMoveDown = {
                                val newOrder = moveTopLevelUnit(entries, row.unitIndex, row.unitIndex + 1)
                                applyReorder(newOrder)
                            },
                            onDragStart = {
                                draggingKey = folderKey
                                dragOffsetY = 0f
                            },
                            onDragDelta = { dy ->
                                dragOffsetY += dy
                                val currentKey = draggingKey ?: return@SidebarFolderHeader
                                if (currentKey != folderKey) return@SidebarFolderHeader
                                val currentUnitIndex = partitionIntoUnits(entries).indexOfFirst {
                                    it is SidebarUnit.GroupUnit && it.groupName.equals(row.groupName, ignoreCase = true)
                                }
                                if (currentUnitIndex == -1) return@SidebarFolderHeader
                                val totalUnits = partitionIntoUnits(entries).size
                                val offsetSteps = (dragOffsetY / itemHeightPx).toInt()
                                val targetUnitIndex = (currentUnitIndex + offsetSteps).coerceIn(0, totalUnits - 1)
                                if (targetUnitIndex != currentUnitIndex) {
                                    val newOrder = moveTopLevelUnit(entries, currentUnitIndex, targetUnitIndex)
                                    applyReorder(newOrder)
                                    dragOffsetY -= (targetUnitIndex - currentUnitIndex) * itemHeightPx
                                }
                            },
                            onDragEnd = {
                                draggingKey = null
                                dragOffsetY = 0f
                            },
                            onToggleCollapse = {
                                collapsedGroups = if (row.isCollapsed) {
                                    collapsedGroups.filterNot { it.equals(row.groupName, ignoreCase = true) }.toSet()
                                } else {
                                    collapsedGroups + row.groupName
                                }
                            },
                            onClick = {
                                if (row.isCollapsed) {
                                    collapsedGroups = collapsedGroups.filterNot { it.equals(row.groupName, ignoreCase = true) }.toSet()
                                    if (!row.hasSelectedChild) {
                                        row.firstChild?.let { onEntrySelected(it) }
                                    }
                                } else {
                                    if (!row.hasSelectedChild) {
                                        row.firstChild?.let { onEntrySelected(it) }
                                    } else {
                                        collapsedGroups = collapsedGroups + row.groupName
                                    }
                                }
                            },
                            modifier = itemModifier,
                        )
                    }
                    is SidebarRow.EntryRow -> {
                        val entryKey = "entry_${row.entry.name}"
                        val isDragging = draggingKey == entryKey
                        val itemModifier = if (isDragging) {
                            Modifier
                                .zIndex(10f)
                                .offset { IntOffset(0, dragOffsetY.roundToInt()) }
                                .shadow(8.dp, RoundedCornerShape(8.dp))
                        } else {
                            Modifier.zIndex(1f)
                        }

                        val canMoveUp = if (row.isNested) row.siblingIndex > 0 else row.unitIndex > 0
                        val canMoveDown = if (row.isNested) row.siblingIndex < row.totalSiblings - 1 else row.unitIndex < row.totalUnits - 1

                        SidebarItem(
                            entry = row.entry,
                            isSelected = row.entry == selectedEntry,
                            isDragging = isDragging,
                            canMoveUp = canMoveUp,
                            canMoveDown = canMoveDown,
                            onMoveUp = {
                                val newOrder = if (row.isNested && row.groupName != null) {
                                    moveChildEntry(entries, row.groupName, row.siblingIndex, row.siblingIndex - 1)
                                } else {
                                    moveTopLevelUnit(entries, row.unitIndex, row.unitIndex - 1)
                                }
                                applyReorder(newOrder)
                            },
                            onMoveDown = {
                                val newOrder = if (row.isNested && row.groupName != null) {
                                    moveChildEntry(entries, row.groupName, row.siblingIndex, row.siblingIndex + 1)
                                } else {
                                    moveTopLevelUnit(entries, row.unitIndex, row.unitIndex + 1)
                                }
                                applyReorder(newOrder)
                            },
                            onDragStart = {
                                draggingKey = entryKey
                                dragOffsetY = 0f
                            },
                            onDragDelta = { dy ->
                                dragOffsetY += dy
                                val currentKey = draggingKey ?: return@SidebarItem
                                if (currentKey != entryKey) return@SidebarItem

                                if (row.isNested && row.groupName != null) {
                                    val groupUnit = partitionIntoUnits(entries).firstOrNull {
                                        it is SidebarUnit.GroupUnit && it.groupName.equals(row.groupName, ignoreCase = true)
                                    } as? SidebarUnit.GroupUnit ?: return@SidebarItem
                                    val currentSiblingIndex = groupUnit.entries.indexOfFirst { it.name == row.entry.name }
                                    if (currentSiblingIndex == -1) return@SidebarItem
                                    val offsetSteps = (dragOffsetY / itemHeightPx).toInt()
                                    val targetSiblingIndex = (currentSiblingIndex + offsetSteps).coerceIn(0, groupUnit.entries.lastIndex)
                                    if (targetSiblingIndex != currentSiblingIndex) {
                                        val newOrder = moveChildEntry(entries, row.groupName, currentSiblingIndex, targetSiblingIndex)
                                        applyReorder(newOrder)
                                        dragOffsetY -= (targetSiblingIndex - currentSiblingIndex) * itemHeightPx
                                    }
                                } else {
                                    val currentUnitIndex = partitionIntoUnits(entries).indexOfFirst {
                                        it is SidebarUnit.StandaloneUnit && it.entry.name == row.entry.name
                                    }
                                    if (currentUnitIndex == -1) return@SidebarItem
                                    val totalUnits = partitionIntoUnits(entries).size
                                    val offsetSteps = (dragOffsetY / itemHeightPx).toInt()
                                    val targetUnitIndex = (currentUnitIndex + offsetSteps).coerceIn(0, totalUnits - 1)
                                    if (targetUnitIndex != currentUnitIndex) {
                                        val newOrder = moveTopLevelUnit(entries, currentUnitIndex, targetUnitIndex)
                                        applyReorder(newOrder)
                                        dragOffsetY -= (targetUnitIndex - currentUnitIndex) * itemHeightPx
                                    }
                                }
                            },
                            onDragEnd = {
                                draggingKey = null
                                dragOffsetY = 0f
                            },
                            onClick = { onEntrySelected(row.entry) },
                            steamAvailable = steamAvailable,
                            isSteamRegistered = isSteamRegistered(row.entry),
                            onSteamToggle = { onSteamToggle?.invoke(row.entry) },
                            isNested = row.isNested,
                            modifier = itemModifier,
                        )
                    }
                }
            }
        }
    }
}

sealed interface SidebarUnit {
    data class GroupUnit(val groupName: String, val entries: List<GameEntry>) : SidebarUnit
    data class StandaloneUnit(val entry: GameEntry) : SidebarUnit
}

internal fun partitionIntoUnits(entries: List<GameEntry>): List<SidebarUnit> {
    val units = mutableListOf<SidebarUnit>()
    val seenGroups = mutableSetOf<String>()

    for (entry in entries) {
        val groupName = entry.group?.trim()?.takeIf { it.isNotBlank() }
        if (groupName == null) {
            units.add(SidebarUnit.StandaloneUnit(entry))
        } else {
            val normalized = groupName.lowercase()
            if (seenGroups.add(normalized)) {
                val groupEntries = entries.filter { it.group?.trim().equals(groupName, ignoreCase = true) }
                units.add(SidebarUnit.GroupUnit(groupName, groupEntries))
            }
        }
    }
    return units
}

internal fun flattenUnits(units: List<SidebarUnit>): List<GameEntry> {
    return units.flatMap { unit ->
        when (unit) {
            is SidebarUnit.GroupUnit -> unit.entries
            is SidebarUnit.StandaloneUnit -> listOf(unit.entry)
        }
    }
}

internal fun moveTopLevelUnit(
    entries: List<GameEntry>,
    fromUnitIndex: Int,
    toUnitIndex: Int,
): List<GameEntry> {
    val units = partitionIntoUnits(entries).toMutableList()
    if (fromUnitIndex !in units.indices || toUnitIndex !in units.indices || fromUnitIndex == toUnitIndex) {
        return entries
    }
    val item = units.removeAt(fromUnitIndex)
    units.add(toUnitIndex, item)
    return flattenUnits(units)
}

internal fun moveChildEntry(
    entries: List<GameEntry>,
    groupName: String,
    fromSiblingIndex: Int,
    toSiblingIndex: Int,
): List<GameEntry> {
    val units = partitionIntoUnits(entries).toMutableList()
    val groupUnitIndex = units.indexOfFirst {
        it is SidebarUnit.GroupUnit && it.groupName.equals(groupName, ignoreCase = true)
    }
    if (groupUnitIndex == -1) return entries
    val groupUnit = units[groupUnitIndex] as SidebarUnit.GroupUnit
    val siblingEntries = groupUnit.entries.toMutableList()
    if (fromSiblingIndex !in siblingEntries.indices || toSiblingIndex !in siblingEntries.indices || fromSiblingIndex == toSiblingIndex) {
        return entries
    }
    val moved = siblingEntries.removeAt(fromSiblingIndex)
    siblingEntries.add(toSiblingIndex, moved)
    units[groupUnitIndex] = groupUnit.copy(entries = siblingEntries)
    return flattenUnits(units)
}

sealed interface SidebarRow {
    data class FolderHeader(
        val groupName: String,
        val count: Int,
        val isCollapsed: Boolean,
        val hasSelectedChild: Boolean,
        val firstChild: GameEntry?,
        val unitIndex: Int,
        val totalUnits: Int,
    ) : SidebarRow

    data class EntryRow(
        val entry: GameEntry,
        val isNested: Boolean,
        val groupName: String?,
        val siblingIndex: Int,
        val totalSiblings: Int,
        val unitIndex: Int,
        val totalUnits: Int,
    ) : SidebarRow
}

internal fun computeSidebarRows(
    entries: List<GameEntry>,
    collapsedGroups: Set<String>,
    selectedEntry: GameEntry?,
): List<SidebarRow> {
    val rows = mutableListOf<SidebarRow>()
    val units = partitionIntoUnits(entries)
    val totalUnits = units.size

    units.forEachIndexed { unitIndex, unit ->
        when (unit) {
            is SidebarUnit.StandaloneUnit -> {
                rows.add(
                    SidebarRow.EntryRow(
                        entry = unit.entry,
                        isNested = false,
                        groupName = null,
                        siblingIndex = 0,
                        totalSiblings = 1,
                        unitIndex = unitIndex,
                        totalUnits = totalUnits,
                    )
                )
            }
            is SidebarUnit.GroupUnit -> {
                val groupEntries = unit.entries
                val isCollapsed = collapsedGroups.any { it.equals(unit.groupName, ignoreCase = true) }
                val hasSelectedChild = selectedEntry != null && groupEntries.any { it.name == selectedEntry.name }

                rows.add(
                    SidebarRow.FolderHeader(
                        groupName = unit.groupName,
                        count = groupEntries.size,
                        isCollapsed = isCollapsed,
                        hasSelectedChild = hasSelectedChild,
                        firstChild = groupEntries.firstOrNull(),
                        unitIndex = unitIndex,
                        totalUnits = totalUnits,
                    )
                )

                if (!isCollapsed) {
                    val totalSiblings = groupEntries.size
                    groupEntries.forEachIndexed { siblingIndex, groupEntry ->
                        rows.add(
                            SidebarRow.EntryRow(
                                entry = groupEntry,
                                isNested = true,
                                groupName = unit.groupName,
                                siblingIndex = siblingIndex,
                                totalSiblings = totalSiblings,
                                unitIndex = unitIndex,
                                totalUnits = totalUnits,
                            )
                        )
                    }
                }
            }
        }
    }
    return rows
}

@Composable
private fun SidebarFolderHeader(
    name: String,
    count: Int,
    isCollapsed: Boolean,
    hasSelectedChild: Boolean,
    isDragging: Boolean,
    canMoveUp: Boolean,
    canMoveDown: Boolean,
    onMoveUp: () -> Unit,
    onMoveDown: () -> Unit,
    onDragStart: () -> Unit,
    onDragDelta: (Float) -> Unit,
    onDragEnd: () -> Unit,
    onClick: () -> Unit,
    onToggleCollapse: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val handCursor = remember { PointerIcon(Cursor.getPredefinedCursor(Cursor.HAND_CURSOR)) }
    val moveCursor = remember { PointerIcon(Cursor.getPredefinedCursor(Cursor.MOVE_CURSOR)) }
    val interactionSource = remember { MutableInteractionSource() }
    val isHovered by interactionSource.collectIsHoveredAsState()
    val rotation by animateFloatAsState(if (isCollapsed) -90f else 0f)

    val currentOnDragDelta by rememberUpdatedState(onDragDelta)
    val currentOnDragStart by rememberUpdatedState(onDragStart)
    val currentOnDragEnd by rememberUpdatedState(onDragEnd)

    Row(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .background(
                when {
                    isDragging -> EmuSyncColors.SurfaceSelected.copy(alpha = 0.9f)
                    hasSelectedChild && isCollapsed -> EmuSyncColors.SurfaceSelected.copy(alpha = 0.5f)
                    isHovered -> EmuSyncColors.CardHover
                    else -> Color.Transparent
                }
            )
            .pointerHoverIcon(handCursor)
            .clickable(interactionSource = interactionSource, indication = null) { onClick() }
            .hoverable(interactionSource)
            .padding(horizontal = 8.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // Drag Handle
        Box(
            modifier = Modifier
                .size(24.dp)
                .pointerHoverIcon(moveCursor)
                .pointerInput(name) {
                    detectDragGestures(
                        onDragStart = { currentOnDragStart() },
                        onDrag = { change, dragAmount ->
                            change.consume()
                            currentOnDragDelta(dragAmount.y)
                        },
                        onDragEnd = { currentOnDragEnd() },
                        onDragCancel = { currentOnDragEnd() },
                    )
                },
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                imageVector = Icons.Default.DragIndicator,
                contentDescription = "Drag folder to reorder",
                tint = if (isDragging) EmuSyncColors.Primary else EmuSyncColors.OnSurfaceDim.copy(alpha = if (isHovered) 0.8f else 0.35f),
                modifier = Modifier.size(16.dp),
            )
        }

        Spacer(Modifier.width(4.dp))

        Icon(
            imageVector = Icons.Default.FolderOpen,
            contentDescription = null,
            tint = if (hasSelectedChild) EmuSyncColors.Primary else EmuSyncColors.Primary.copy(alpha = 0.85f),
            modifier = Modifier.size(18.dp),
        )
        Spacer(Modifier.width(8.dp))
        Text(
            text = name,
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.Bold,
            color = if (hasSelectedChild) EmuSyncColors.Primary else EmuSyncColors.OnBackground,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        // Count badge
        Box(
            modifier = Modifier
                .clip(RoundedCornerShape(10.dp))
                .background(EmuSyncColors.SurfaceVariant)
                .padding(horizontal = 6.dp, vertical = 2.dp)
        ) {
            Text(
                text = "$count",
                style = MaterialTheme.typography.labelSmall,
                color = EmuSyncColors.OnSurfaceDim,
                fontWeight = FontWeight.Medium,
            )
        }

        // Up/Down reorder arrows on hover
        if (isHovered && !isDragging) {
            Spacer(Modifier.width(4.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (canMoveUp) {
                    IconButton(
                        onClick = onMoveUp,
                        modifier = Modifier.size(20.dp),
                    ) {
                        Icon(
                            imageVector = Icons.Default.KeyboardArrowUp,
                            contentDescription = "Move folder up",
                            tint = EmuSyncColors.OnSurfaceDim,
                            modifier = Modifier.size(14.dp),
                        )
                    }
                }
                if (canMoveDown) {
                    IconButton(
                        onClick = onMoveDown,
                        modifier = Modifier.size(20.dp),
                    ) {
                        Icon(
                            imageVector = Icons.Default.KeyboardArrowDown,
                            contentDescription = "Move folder down",
                            tint = EmuSyncColors.OnSurfaceDim,
                            modifier = Modifier.size(14.dp),
                        )
                    }
                }
            }
        }

        Spacer(Modifier.width(4.dp))
        IconButton(
            onClick = onToggleCollapse,
            modifier = Modifier.size(24.dp),
        ) {
            Icon(
                imageVector = Icons.Default.KeyboardArrowDown,
                contentDescription = if (isCollapsed) "Expand" else "Collapse",
                tint = EmuSyncColors.OnSurfaceDim,
                modifier = Modifier
                    .size(18.dp)
                    .rotate(rotation),
            )
        }
    }
}

@Composable
private fun SidebarItem(
    entry: GameEntry,
    isSelected: Boolean,
    isDragging: Boolean,
    canMoveUp: Boolean,
    canMoveDown: Boolean,
    onMoveUp: () -> Unit,
    onMoveDown: () -> Unit,
    onDragDelta: (Float) -> Unit,
    onDragStart: () -> Unit,
    onDragEnd: () -> Unit,
    onClick: () -> Unit,
    steamAvailable: Boolean = false,
    isSteamRegistered: Boolean = false,
    onSteamToggle: () -> Unit = {},
    isNested: Boolean = false,
    modifier: Modifier = Modifier,
) {
    val currentOnDragDelta by rememberUpdatedState(onDragDelta)
    val currentOnDragStart by rememberUpdatedState(onDragStart)
    val currentOnDragEnd by rememberUpdatedState(onDragEnd)

    val interactionSource = remember { MutableInteractionSource() }
    val isHovered by interactionSource.collectIsHoveredAsState()

    val backgroundColor by animateColorAsState(
        when {
            isDragging -> EmuSyncColors.SurfaceSelected.copy(alpha = 0.9f)
            isSelected -> EmuSyncColors.SurfaceSelected
            isHovered -> EmuSyncColors.CardHover
            else -> EmuSyncColors.Surface
        }
    )

    val textColor by animateColorAsState(
        when {
            isSelected -> EmuSyncColors.Primary
            isHovered -> EmuSyncColors.OnBackground
            else -> EmuSyncColors.OnSurface
        }
    )

    val icon = when (entry) {
        is EmulatorSystem -> Icons.Default.SportsEsports
        is NativePCGame -> Icons.Default.Gamepad
    }

    val subtitle = when (entry) {
        is EmulatorSystem -> entry.extensions.joinToString(", ") { ".$it" }
        is NativePCGame -> "Native"
    }

    val moveCursor = remember { PointerIcon(Cursor.getPredefinedCursor(Cursor.MOVE_CURSOR)) }
    val handCursor = remember { PointerIcon(Cursor.getPredefinedCursor(Cursor.HAND_CURSOR)) }

    Row(
        modifier = modifier
            .fillMaxWidth()
            .then(if (isNested) Modifier.padding(start = 12.dp) else Modifier)
            .clip(RoundedCornerShape(8.dp))
            .background(backgroundColor)
            .pointerHoverIcon(handCursor)
            .clickable(interactionSource = interactionSource, indication = null) { onClick() }
            .hoverable(interactionSource)
            .padding(horizontal = 8.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // Drag Handle
        Box(
            modifier = Modifier
                .size(24.dp)
                .pointerHoverIcon(moveCursor)
                .pointerInput(entry.name) {
                    detectDragGestures(
                        onDragStart = { currentOnDragStart() },
                        onDrag = { change, dragAmount ->
                            change.consume()
                            currentOnDragDelta(dragAmount.y)
                        },
                        onDragEnd = { currentOnDragEnd() },
                        onDragCancel = { currentOnDragEnd() },
                    )
                },
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                imageVector = Icons.Default.DragIndicator,
                contentDescription = "Drag to reorder",
                tint = if (isDragging) EmuSyncColors.Primary else EmuSyncColors.OnSurfaceDim.copy(alpha = if (isHovered) 0.8f else 0.35f),
                modifier = Modifier.size(16.dp),
            )
        }

        Spacer(Modifier.width(4.dp))

        Icon(
            imageVector = icon,
            contentDescription = null,
            tint = textColor,
            modifier = Modifier.size(20.dp),
        )
        Spacer(Modifier.width(8.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = entry.name,
                color = textColor,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = if (isSelected) FontWeight.SemiBold else FontWeight.Normal,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = subtitle,
                color = EmuSyncColors.OnSurfaceDim,
                style = MaterialTheme.typography.labelSmall,
                maxLines = 1,
            )
        }

        // Up/Down reorder arrows on hover
        if (isHovered && !isDragging) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (canMoveUp) {
                    IconButton(
                        onClick = onMoveUp,
                        modifier = Modifier.size(20.dp),
                    ) {
                        Icon(
                            imageVector = Icons.Default.KeyboardArrowUp,
                            contentDescription = "Move up",
                            tint = EmuSyncColors.OnSurfaceDim,
                            modifier = Modifier.size(14.dp),
                        )
                    }
                }
                if (canMoveDown) {
                    IconButton(
                        onClick = onMoveDown,
                        modifier = Modifier.size(20.dp),
                    ) {
                        Icon(
                            imageVector = Icons.Default.KeyboardArrowDown,
                            contentDescription = "Move down",
                            tint = EmuSyncColors.OnSurfaceDim,
                            modifier = Modifier.size(14.dp),
                        )
                    }
                }
            }
        }

        // Steam registration toggle button
        if (steamAvailable) {
            val steamColor by animateColorAsState(
                if (isSteamRegistered) Color(0xFF66BB6A) else EmuSyncColors.OnSurfaceDim.copy(alpha = 0.4f)
            )

            IconButton(
                onClick = onSteamToggle,
                modifier = Modifier.size(24.dp),
            ) {
                Icon(
                    imageVector = Icons.Default.Widgets,
                    contentDescription = if (isSteamRegistered) "Unregister from Steam" else "Register in Steam",
                    tint = steamColor,
                    modifier = Modifier.size(16.dp),
                )
            }
            Spacer(Modifier.width(4.dp))
        }

        // Selection indicator bar
        if (isSelected) {
            Box(
                modifier = Modifier
                    .width(3.dp)
                    .height(24.dp)
                    .clip(RoundedCornerShape(2.dp))
                    .background(EmuSyncColors.Primary),
            )
        }
    }
}
