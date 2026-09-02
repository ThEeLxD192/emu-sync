package com.emusync.ui.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Gamepad
import androidx.compose.material.icons.filled.SportsEsports
import androidx.compose.material.icons.filled.Widgets
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.emusync.model.EmulatorSystem
import com.emusync.model.GameEntry
import com.emusync.model.NativePCGame
import com.emusync.ui.theme.EmuSyncColors

/**
 * Sidebar showing the list of categories (emulator systems and native games).
 * Each item is clickable and highlights when selected or hovered.
 * Includes a Steam registration toggle button on each entry.
 */
@Composable
fun Sidebar(
    entries: List<GameEntry>,
    selectedEntry: GameEntry?,
    onEntrySelected: (GameEntry) -> Unit,
    onAddClicked: () -> Unit,
    steamAvailable: Boolean = false,
    isSteamRegistered: (GameEntry) -> Boolean = { it.steamAppId != null },
    onSteamToggle: ((GameEntry) -> Unit)? = null,
    modifier: Modifier = Modifier,
) {
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
            items(entries, key = { it.name }) { entry ->
                SidebarItem(
                    entry = entry,
                    isSelected = entry == selectedEntry,
                    onClick = { onEntrySelected(entry) },
                    steamAvailable = steamAvailable,
                    isSteamRegistered = isSteamRegistered(entry),
                    onSteamToggle = { onSteamToggle?.invoke(entry) },
                )
            }
        }
    }
}

@Composable
private fun SidebarItem(
    entry: GameEntry,
    isSelected: Boolean,
    onClick: () -> Unit,
    steamAvailable: Boolean = false,
    isSteamRegistered: Boolean = false,
    onSteamToggle: () -> Unit = {},
) {
    val interactionSource = remember { MutableInteractionSource() }
    val isHovered by interactionSource.collectIsHoveredAsState()

    val backgroundColor by animateColorAsState(
        when {
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

    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .background(backgroundColor)
            .clickable(interactionSource = interactionSource, indication = null) { onClick() }
            .hoverable(interactionSource)
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            tint = textColor,
            modifier = Modifier.size(20.dp),
        )
        Spacer(Modifier.width(10.dp))
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
