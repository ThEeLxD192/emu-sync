package com.emusync.ui.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.emusync.model.EmulatorSystem
import com.emusync.model.GameEntry
import com.emusync.model.NativePCGame
import com.emusync.ui.GameItem
import com.emusync.ui.theme.EmuSyncColors

/**
 * Game grid that fills the main content area.
 * Shows game cards in a responsive grid layout.
 */
@Composable
fun GameGrid(
    items: List<GameItem>,
    selectedEntry: GameEntry?,
    isLoading: Boolean,
    onGameClicked: (GameItem) -> Unit,
    onEditGame: (GameItem) -> Unit = {},
    onEditCategory: () -> Unit = {},
    onDeleteCategory: () -> Unit = {},
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier) {
        // Header with category name and game count
        if (selectedEntry != null) {
            Row(
                modifier = Modifier.fillMaxWidth().padding(bottom = 12.dp),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = selectedEntry.name,
                    style = MaterialTheme.typography.titleLarge,
                    fontWeight = FontWeight.Bold,
                    color = EmuSyncColors.OnBackground,
                )
                Row(verticalAlignment = Alignment.CenterVertically) {
                    IconButton(onClick = onEditCategory) {
                        Icon(
                            imageVector = Icons.Default.Edit,
                            contentDescription = "Edit Category",
                            tint = EmuSyncColors.OnSurfaceDim
                        )
                    }
                    IconButton(onClick = onDeleteCategory) {
                        Icon(
                            imageVector = Icons.Default.Delete,
                            contentDescription = "Delete Category",
                            tint = EmuSyncColors.OnSurfaceDim
                        )
                    }
                    Spacer(Modifier.width(8.dp))
                    Surface(
                        shape = RoundedCornerShape(12.dp),
                        color = EmuSyncColors.PrimaryContainer,
                    ) {
                        Text(
                            text = "${items.size} game${if (items.size != 1) "s" else ""}",
                            style = MaterialTheme.typography.labelMedium,
                            color = EmuSyncColors.Primary,
                            modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
                        )
                    }
                }
            }
        }

        if (isLoading) {
            Box(
                modifier = Modifier.fillMaxSize(),
                contentAlignment = Alignment.Center,
            ) {
                CircularProgressIndicator()
            }
        }

        if (items.isEmpty() && !isLoading) {
            // Empty state
            Box(
                modifier = Modifier.fillMaxSize(),
                contentAlignment = Alignment.Center,
            ) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Text(
                        text = if (selectedEntry is EmulatorSystem)
                            "No ROMs found"
                        else
                            "Select a category",
                        style = MaterialTheme.typography.titleMedium,
                        color = EmuSyncColors.OnSurfaceDim,
                    )
                    if (selectedEntry is EmulatorSystem) {
                        Spacer(Modifier.height(4.dp))
                        Text(
                            text = "Add ROMs to: ${selectedEntry.romsDirectory}",
                            style = MaterialTheme.typography.bodySmall,
                            color = EmuSyncColors.OnSurfaceDim,
                        )
                    }
                }
            }
        } else if (!isLoading) {
            LazyVerticalGrid(
                columns = GridCells.Adaptive(minSize = 180.dp),
                contentPadding = PaddingValues(bottom = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                items(items, key = { gameItem -> "${gameItem.entry.name}_${gameItem.romFile?.absolutePath ?: gameItem.name}" }) { gameItem ->
                    GameCard(
                        item = gameItem,
                        onClick = { onGameClicked(gameItem) },
                        onEdit = if (gameItem.entry is EmulatorSystem) { { onEditGame(gameItem) } } else null,
                    )
                }
            }
        }
    }
}

@Composable
private fun GameCard(
    item: GameItem,
    onClick: () -> Unit,
    onEdit: (() -> Unit)? = null,
) {
    val interactionSource = remember { MutableInteractionSource() }
    val isHovered by interactionSource.collectIsHoveredAsState()

    val cardColor by animateColorAsState(
        if (isHovered) EmuSyncColors.CardHover else EmuSyncColors.SurfaceVariant
    )

    val borderColor by animateColorAsState(
        if (isHovered) EmuSyncColors.Primary else EmuSyncColors.Divider
    )

    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .height(148.dp)
            .clip(RoundedCornerShape(12.dp))
            .clickable(interactionSource = interactionSource, indication = null) { onClick() }
            .hoverable(interactionSource),
        shape = RoundedCornerShape(12.dp),
        color = cardColor,
        border = androidx.compose.foundation.BorderStroke(1.dp, borderColor),
    ) {
        Box(
            modifier = Modifier.fillMaxSize().padding(10.dp)
        ) {
            // Top-right edit button
            if (onEdit != null) {
                IconButton(
                    onClick = { onEdit() },
                    modifier = Modifier
                        .align(Alignment.TopEnd)
                        .size(26.dp)
                ) {
                    Icon(
                        imageVector = Icons.Default.Edit,
                        contentDescription = "Edit Saves",
                        tint = EmuSyncColors.OnSurfaceDim,
                        modifier = Modifier.size(15.dp)
                    )
                }
            }

            // Compact centered content: Icon + Title + Format tag
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .align(Alignment.Center)
                    .padding(horizontal = 8.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center,
            ) {
                // Game icon
                Box(
                    modifier = Modifier
                        .size(42.dp)
                        .clip(RoundedCornerShape(10.dp))
                        .background(EmuSyncColors.PrimaryContainer),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(
                        imageVector = Icons.Default.PlayArrow,
                        contentDescription = "Launch",
                        tint = EmuSyncColors.Primary,
                        modifier = Modifier.size(24.dp),
                    )
                }

                Spacer(Modifier.height(8.dp))

                // Game name
                Text(
                    text = item.name,
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.Medium,
                    color = EmuSyncColors.OnBackground,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    textAlign = TextAlign.Center,
                )

                // Extension or Native tag
                Spacer(Modifier.height(4.dp))
                val label = when (item.entry) {
                    is EmulatorSystem -> item.romFile?.extension?.uppercase() ?: ""
                    is NativePCGame -> "NATIVE"
                }
                if (label.isNotEmpty()) {
                    Text(
                        text = label,
                        style = MaterialTheme.typography.labelSmall,
                        color = EmuSyncColors.OnSurfaceDim,
                    )
                }
            }
        }
    }
}
