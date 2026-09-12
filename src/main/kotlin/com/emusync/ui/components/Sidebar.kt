package com.emusync.ui.components

import androidx.compose.animation.animateColorAsState
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
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.DragIndicator
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
    steamAvailable: Boolean = false,
    isSteamRegistered: (GameEntry) -> Boolean = { it.steamAppId != null },
    onSteamToggle: ((GameEntry) -> Unit)? = null,
    modifier: Modifier = Modifier,
) {
    var draggingEntryName by remember { mutableStateOf<String?>(null) }
    var dragOffsetY by remember { mutableStateOf(0f) }
    val density = LocalDensity.current
    val itemHeightPx = with(density) { 52.dp.toPx() }

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
            itemsIndexed(entries, key = { _, it -> it.name }) { index, entry ->
                val isDragging = draggingEntryName == entry.name

                val itemModifier = if (isDragging) {
                    Modifier
                        .zIndex(10f)
                        .offset { IntOffset(0, dragOffsetY.roundToInt()) }
                        .shadow(8.dp, RoundedCornerShape(8.dp))
                } else {
                    Modifier.zIndex(1f)
                }

                SidebarItem(
                    entry = entry,
                    isSelected = entry == selectedEntry,
                    isDragging = isDragging,
                    canMoveUp = index > 0,
                    canMoveDown = index < entries.lastIndex,
                    onMoveUp = { onReorder?.invoke(index, index - 1) },
                    onMoveDown = { onReorder?.invoke(index, index + 1) },
                    onDragStart = {
                        draggingEntryName = entry.name
                        dragOffsetY = 0f
                    },
                    onDragDelta = { dy ->
                        dragOffsetY += dy
                        val currentName = draggingEntryName ?: return@SidebarItem
                        val currentIndex = entries.indexOfFirst { it.name == currentName }
                        if (currentIndex == -1) return@SidebarItem
                        val offsetSteps = (dragOffsetY / itemHeightPx).toInt()
                        val targetIndex = (currentIndex + offsetSteps).coerceIn(0, entries.lastIndex)
                        if (targetIndex != currentIndex) {
                            onReorder?.invoke(currentIndex, targetIndex)
                            dragOffsetY -= (targetIndex - currentIndex) * itemHeightPx
                        }
                    },
                    onDragEnd = {
                        draggingEntryName = null
                        dragOffsetY = 0f
                    },
                    onClick = { onEntrySelected(entry) },
                    steamAvailable = steamAvailable,
                    isSteamRegistered = isSteamRegistered(entry),
                    onSteamToggle = { onSteamToggle?.invoke(entry) },
                    modifier = itemModifier,
                )
            }
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

    Row(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .background(backgroundColor)
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
