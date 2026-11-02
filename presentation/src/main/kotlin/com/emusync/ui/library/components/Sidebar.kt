package com.emusync.ui.library.components

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
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
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
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.PointerIcon
import androidx.compose.ui.input.pointer.pointerHoverIcon
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import com.emusync.ui.theme.EmuSyncColors
import java.awt.Cursor

/**
 * Sidebar showing the list of categories (emulator systems and native games). Supports
 * drag-and-drop reordering as well as quick up/down controls.
 */
@Composable
fun Sidebar(
    entries: List<GameEntry>,
    selectedEntry: GameEntry?,
    onEntrySelected: (GameEntry) -> Unit,
    onAddClicked: () -> Unit,
    onReorderEntries: (List<GameEntry>) -> Unit,
    steamAvailable: Boolean = false,
    isSteamRegistered: (GameEntry) -> Boolean = { it.steamAppId != null },
    onSteamToggle: ((GameEntry) -> Unit)? = null,
    modifier: Modifier = Modifier,
) {
    val interaction = remember { SidebarInteractionState() }
    val density = LocalDensity.current
    val itemHeightPx = with(density) { 52.dp.toPx() }

    val sidebarRows =
        remember(entries, interaction.collapsedGroups, selectedEntry) {
            computeSidebarRows(entries, interaction.collapsedGroups, selectedEntry)
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
                key = { row -> row.key },
            ) { row ->
                when (row) {
                    is SidebarRow.FolderHeader ->
                        SidebarGroupRow(
                            row,
                            entries,
                            interaction,
                            itemHeightPx,
                            onReorderEntries,
                            onEntrySelected,
                        )
                    is SidebarRow.EntryRow ->
                        SidebarEntryRow(
                            row,
                            entries,
                            selectedEntry,
                            interaction,
                            itemHeightPx,
                            onReorderEntries,
                            onEntrySelected,
                            steamAvailable,
                            isSteamRegistered,
                            onSteamToggle,
                        )
                }
            }
        }
    }
}

@Composable
private fun SidebarGroupRow(
    row: SidebarRow.FolderHeader,
    entries: List<GameEntry>,
    interaction: SidebarInteractionState,
    itemHeightPx: Float,
    applyReorder: (List<GameEntry>) -> Unit,
    onEntrySelected: (GameEntry) -> Unit,
) {
    SidebarFolderHeader(
        name = row.groupName,
        count = row.count,
        isCollapsed = row.isCollapsed,
        hasSelectedChild = row.hasSelectedChild,
        isDragging = interaction.draggingKey == row.key,
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
        onDragStart = { interaction.startDrag(row.key) },
        onDragDelta = { dy ->
            interaction.dragFolder(entries, row, dy, itemHeightPx, applyReorder)
        },
        onDragEnd = { interaction.endDrag() },
        onToggleCollapse = { interaction.toggleGroup(row) },
        onClick = { interaction.activateFolder(row, onEntrySelected) },
        modifier = interaction.rowModifier(row.key),
    )
}

@Composable
private fun SidebarEntryRow(
    row: SidebarRow.EntryRow,
    entries: List<GameEntry>,
    selectedEntry: GameEntry?,
    interaction: SidebarInteractionState,
    itemHeightPx: Float,
    applyReorder: (List<GameEntry>) -> Unit,
    onEntrySelected: (GameEntry) -> Unit,
    steamAvailable: Boolean,
    isSteamRegistered: (GameEntry) -> Boolean,
    onSteamToggle: ((GameEntry) -> Unit)?,
) {
    val position = if (row.isNested) row.siblingIndex else row.unitIndex
    val count = if (row.isNested) row.totalSiblings else row.totalUnits
    SidebarItem(
        entry = row.entry,
        isSelected = row.entry == selectedEntry,
        isDragging = interaction.draggingKey == row.key,
        canMoveUp = position > 0,
        canMoveDown = position < count - 1,
        onMoveUp = { applyReorder(moveSidebarEntry(entries, row, -1)) },
        onMoveDown = { applyReorder(moveSidebarEntry(entries, row, 1)) },
        onDragStart = { interaction.startDrag(row.key) },
        onDragDelta = { dy -> interaction.dragEntry(entries, row, dy, itemHeightPx, applyReorder) },
        onDragEnd = { interaction.endDrag() },
        onClick = { onEntrySelected(row.entry) },
        steamAvailable = steamAvailable,
        isSteamRegistered = isSteamRegistered(row.entry),
        onSteamToggle = { onSteamToggle?.invoke(row.entry) },
        isNested = row.isNested,
        modifier = interaction.rowModifier(row.key),
    )
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
    val interactionSource = remember { MutableInteractionSource() }
    val isHovered by interactionSource.collectIsHoveredAsState()
    val rotation by animateFloatAsState(if (isCollapsed) -90f else 0f)

    Row(
        modifier =
            modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(8.dp))
                .background(
                    when {
                        isDragging -> EmuSyncColors.SurfaceSelected.copy(alpha = 0.9f)
                        hasSelectedChild && isCollapsed ->
                            EmuSyncColors.SurfaceSelected.copy(alpha = 0.5f)
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
        SidebarDragHandle(
            name,
            isDragging,
            isHovered,
            onDragStart,
            onDragDelta,
            onDragEnd,
            "Drag folder to reorder",
        )

        Spacer(Modifier.width(4.dp))

        Icon(
            imageVector = Icons.Default.FolderOpen,
            contentDescription = null,
            tint =
                if (hasSelectedChild) EmuSyncColors.Primary
                else EmuSyncColors.Primary.copy(alpha = 0.85f),
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
            modifier =
                Modifier.clip(RoundedCornerShape(10.dp))
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

        ReorderButtons(
            isHovered && !isDragging,
            canMoveUp,
            canMoveDown,
            onMoveUp,
            onMoveDown,
            "Move folder up",
            "Move folder down",
            modifier = Modifier.padding(start = 4.dp),
        )

        Spacer(Modifier.width(4.dp))
        IconButton(
            onClick = onToggleCollapse,
            modifier = Modifier.size(24.dp),
        ) {
            Icon(
                imageVector = Icons.Default.KeyboardArrowDown,
                contentDescription = if (isCollapsed) "Expand" else "Collapse",
                tint = EmuSyncColors.OnSurfaceDim,
                modifier = Modifier.size(18.dp).rotate(rotation),
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

    val interactionSource = remember { MutableInteractionSource() }
    val isHovered by interactionSource.collectIsHoveredAsState()

    val backgroundColor by
        animateColorAsState(
            when {
                isDragging -> EmuSyncColors.SurfaceSelected.copy(alpha = 0.9f)
                isSelected -> EmuSyncColors.SurfaceSelected
                isHovered -> EmuSyncColors.CardHover
                else -> EmuSyncColors.Surface
            }
        )

    val textColor by
        animateColorAsState(
            when {
                isSelected -> EmuSyncColors.Primary
                isHovered -> EmuSyncColors.OnBackground
                else -> EmuSyncColors.OnSurface
            }
        )

    val handCursor = remember { PointerIcon(Cursor.getPredefinedCursor(Cursor.HAND_CURSOR)) }

    Row(
        modifier =
            modifier
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
        SidebarDragHandle(
            entry.name,
            isDragging,
            isHovered,
            onDragStart,
            onDragDelta,
            onDragEnd,
            "Drag to reorder",
        )

        Spacer(Modifier.width(4.dp))

        SidebarEntryLabel(entry, isSelected, textColor)

        ReorderButtons(
            isHovered && !isDragging,
            canMoveUp,
            canMoveDown,
            onMoveUp,
            onMoveDown,
            "Move up",
            "Move down",
        )

        if (steamAvailable) SteamRegistrationToggle(isSteamRegistered, onSteamToggle)

        // Selection indicator bar
        if (isSelected) {
            Box(
                modifier =
                    Modifier.width(3.dp)
                        .height(24.dp)
                        .clip(RoundedCornerShape(2.dp))
                        .background(EmuSyncColors.Primary)
            )
        }
    }
}

@Composable
private fun SidebarDragHandle(
    key: String,
    isDragging: Boolean,
    isHovered: Boolean,
    onDragStart: () -> Unit,
    onDragDelta: (Float) -> Unit,
    onDragEnd: () -> Unit,
    description: String,
) {
    val moveCursor = remember { PointerIcon(Cursor.getPredefinedCursor(Cursor.MOVE_CURSOR)) }
    val currentOnDragDelta by rememberUpdatedState(onDragDelta)
    val currentOnDragStart by rememberUpdatedState(onDragStart)
    val currentOnDragEnd by rememberUpdatedState(onDragEnd)
    // Drag Handle
    Box(
        modifier =
            Modifier.size(24.dp).pointerHoverIcon(moveCursor).pointerInput(key) {
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
            contentDescription = description,
            tint =
                if (isDragging) EmuSyncColors.Primary
                else EmuSyncColors.OnSurfaceDim.copy(alpha = if (isHovered) 0.8f else 0.35f),
            modifier = Modifier.size(16.dp),
        )
    }
}

@Composable
private fun RowScope.SidebarEntryLabel(entry: GameEntry, isSelected: Boolean, textColor: Color) {
    val icon =
        when (entry) {
            is EmulatorSystem -> Icons.Default.SportsEsports
            is NativePCGame -> Icons.Default.Gamepad
        }

    val subtitle =
        when (entry) {
            is EmulatorSystem -> entry.extensions.joinToString(", ") { ".$it" }
            is NativePCGame -> "Native"
        }

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
}

@Composable
private fun SteamRegistrationToggle(isSteamRegistered: Boolean, onSteamToggle: () -> Unit) {

    val steamColor by
        animateColorAsState(
            if (isSteamRegistered) Color(0xFF66BB6A)
            else EmuSyncColors.OnSurfaceDim.copy(alpha = 0.4f)
        )

    IconButton(
        onClick = onSteamToggle,
        modifier = Modifier.size(24.dp),
    ) {
        Icon(
            imageVector = Icons.Default.Widgets,
            contentDescription =
                if (isSteamRegistered) "Unregister from Steam" else "Register in Steam",
            tint = steamColor,
            modifier = Modifier.size(16.dp),
        )
    }
    Spacer(Modifier.width(4.dp))
}

@Composable
private fun ReorderButtons(
    visible: Boolean,
    canMoveUp: Boolean,
    canMoveDown: Boolean,
    onMoveUp: () -> Unit,
    onMoveDown: () -> Unit,
    upDescription: String,
    downDescription: String,
    modifier: Modifier = Modifier,
) {
    if (visible)
        Row(modifier = modifier, verticalAlignment = Alignment.CenterVertically) {
            if (canMoveUp)
                IconButton(onClick = onMoveUp, modifier = Modifier.size(20.dp)) {
                    Icon(
                        Icons.Default.KeyboardArrowUp,
                        contentDescription = upDescription,
                        tint = EmuSyncColors.OnSurfaceDim,
                        modifier = Modifier.size(14.dp),
                    )
                }
            if (canMoveDown)
                IconButton(onClick = onMoveDown, modifier = Modifier.size(20.dp)) {
                    Icon(
                        Icons.Default.KeyboardArrowDown,
                        contentDescription = downDescription,
                        tint = EmuSyncColors.OnSurfaceDim,
                        modifier = Modifier.size(14.dp),
                    )
                }
        }
}
