package com.emusync.ui.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.*
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.Image
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.toComposeImageBitmap
import androidx.compose.ui.layout.ContentScale
import java.io.File
import com.emusync.model.EmulatorSystem
import com.emusync.model.GameEntry
import com.emusync.model.NativePCGame
import com.emusync.ui.CloudSyncStatus
import com.emusync.ui.GameItem
import com.emusync.ui.theme.EmuSyncColors

/**
 * Game grid that fills the main content area.
 * Shows game cards in a responsive grid layout with category header and Steam-style cloud sync status.
 */
@Composable
fun GameGrid(
    items: List<GameItem>,
    selectedEntry: GameEntry?,
    isLoading: Boolean,
    syncStatus: CloudSyncStatus = CloudSyncStatus.IDLE,
    onGameClicked: (GameItem) -> Unit,
    onSyncCategory: (() -> Unit)? = null,
    onCheckSync: (() -> Unit)? = null,
    onEditGame: (GameItem) -> Unit = {},
    onEditCategory: () -> Unit = {},
    onDeleteCategory: () -> Unit = {},
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier) {
        // Header with category name, game count, and Steam Cloud-like status
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
                    // Cloud Status Indicator & Manual Sync Button
                    if (onSyncCategory != null) {
                        CloudStatusChip(
                            status = syncStatus,
                            onSyncClick = onSyncCategory,
                            onCheckClick = onCheckSync,
                        )
                        Spacer(Modifier.width(8.dp))
                    }

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
                columns = GridCells.Adaptive(minSize = 160.dp),
                contentPadding = PaddingValues(bottom = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
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
private fun CloudStatusChip(
    status: CloudSyncStatus,
    onSyncClick: () -> Unit,
    onCheckClick: (() -> Unit)?,
) {
    val infiniteTransition = rememberInfiniteTransition()
    val rotation by infiniteTransition.animateFloat(
        initialValue = 0f,
        targetValue = 360f,
        animationSpec = infiniteRepeatable(
            animation = tween(1200, easing = LinearEasing)
        )
    )

    when (status) {
        CloudSyncStatus.IN_SYNC -> {
            Surface(
                shape = RoundedCornerShape(8.dp),
                color = EmuSyncColors.SurfaceVariant,
                modifier = Modifier.padding(end = 4.dp)
            ) {
                Row(
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Icon(
                        imageVector = Icons.Default.CloudDone,
                        contentDescription = "Partidas sincronizadas",
                        tint = EmuSyncColors.Success,
                        modifier = Modifier.size(16.dp)
                    )
                    Spacer(Modifier.width(6.dp))
                    Text(
                        text = "Sincronizado",
                        style = MaterialTheme.typography.labelSmall,
                        color = EmuSyncColors.OnSurface
                    )
                    if (onCheckClick != null) {
                        Spacer(Modifier.width(4.dp))
                        IconButton(
                            onClick = onCheckClick,
                            modifier = Modifier.size(18.dp)
                        ) {
                            Icon(
                                imageVector = Icons.Default.Refresh,
                                contentDescription = "Comprobar",
                                tint = EmuSyncColors.OnSurfaceDim,
                                modifier = Modifier.size(13.dp)
                            )
                        }
                    }
                }
            }
        }
        CloudSyncStatus.OUT_OF_SYNC, CloudSyncStatus.CONFLICT -> {
            Button(
                onClick = onSyncClick,
                colors = ButtonDefaults.buttonColors(
                    containerColor = if (status == CloudSyncStatus.CONFLICT) EmuSyncColors.Warning.copy(alpha = 0.2f) else EmuSyncColors.PrimaryContainer,
                    contentColor = if (status == CloudSyncStatus.CONFLICT) EmuSyncColors.Warning else EmuSyncColors.Primary,
                ),
                contentPadding = PaddingValues(horizontal = 10.dp, vertical = 6.dp),
                shape = RoundedCornerShape(8.dp),
                modifier = Modifier.padding(end = 4.dp)
            ) {
                Icon(
                    imageVector = if (status == CloudSyncStatus.CONFLICT) Icons.Default.Warning else Icons.Default.CloudDownload,
                    contentDescription = null,
                    modifier = Modifier.size(16.dp)
                )
                Spacer(Modifier.width(6.dp))
                Text(
                    text = if (status == CloudSyncStatus.CONFLICT) "Conflicto • Sincronizar" else "No sincronizado • Sincronizar todo",
                    style = MaterialTheme.typography.labelSmall,
                    fontWeight = FontWeight.Medium
                )
            }
        }
        CloudSyncStatus.CHECKING, CloudSyncStatus.SYNCING -> {
            Surface(
                shape = RoundedCornerShape(8.dp),
                color = EmuSyncColors.SurfaceVariant,
                modifier = Modifier.padding(end = 4.dp)
            ) {
                Row(
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Icon(
                        imageVector = Icons.Default.CloudSync,
                        contentDescription = null,
                        tint = EmuSyncColors.Primary,
                        modifier = Modifier.size(16.dp).rotate(rotation)
                    )
                    Spacer(Modifier.width(6.dp))
                    Text(
                        text = if (status == CloudSyncStatus.SYNCING) "Sincronizando..." else "Comprobando...",
                        style = MaterialTheme.typography.labelSmall,
                        color = EmuSyncColors.OnSurface
                    )
                }
            }
        }
        CloudSyncStatus.IDLE, CloudSyncStatus.ERROR, CloudSyncStatus.NOT_CONFIGURED -> {
            if (status != CloudSyncStatus.NOT_CONFIGURED) {
                OutlinedButton(
                    onClick = onSyncClick,
                    contentPadding = PaddingValues(horizontal = 10.dp, vertical = 6.dp),
                    shape = RoundedCornerShape(8.dp),
                    modifier = Modifier.padding(end = 4.dp)
                ) {
                    Icon(
                        imageVector = Icons.Default.CloudSync,
                        contentDescription = "Sincronizar todo",
                        tint = EmuSyncColors.Primary,
                        modifier = Modifier.size(16.dp)
                    )
                    Spacer(Modifier.width(6.dp))
                    Text(
                        text = "Sincronizar todo",
                        style = MaterialTheme.typography.labelSmall
                    )
                }
            }
        }
    }
}

private val coverCache = java.util.Collections.synchronizedMap(
    object : LinkedHashMap<String, ImageBitmap>(32, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, ImageBitmap>?): Boolean {
            return size > 100
        }
    }
)

private fun loadGameCover(file: File?): ImageBitmap? {
    if (file == null || !file.exists() || !file.isFile) return null
    val path = file.absolutePath
    coverCache[path]?.let { return it }
    return try {
        val bytes = file.readBytes()
        val bitmap = org.jetbrains.skia.Image.makeFromEncoded(bytes).toComposeImageBitmap()
        coverCache[path] = bitmap
        bitmap
    } catch (_: Throwable) {
        try {
            val bitmap = javax.imageio.ImageIO.read(file)?.toComposeImageBitmap()
            if (bitmap != null) coverCache[path] = bitmap
            bitmap
        } catch (_: Throwable) {
            null
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

    val coverBitmap = remember(item.coverFile?.absolutePath) {
        loadGameCover(item.coverFile)
    }

    val cardColor by animateColorAsState(
        if (isHovered) EmuSyncColors.CardHover else EmuSyncColors.SurfaceVariant
    )

    val borderColor by animateColorAsState(
        if (isHovered) EmuSyncColors.Primary else EmuSyncColors.Divider
    )

    val editAlpha by animateFloatAsState(
        targetValue = if (isHovered) 1f else 0f,
        animationSpec = tween(durationMillis = 150)
    )

    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .height(205.dp)
            .clip(RoundedCornerShape(12.dp))
            .clickable(interactionSource = interactionSource, indication = null) { onClick() }
            .hoverable(interactionSource),
        shape = RoundedCornerShape(12.dp),
        color = cardColor,
        border = androidx.compose.foundation.BorderStroke(1.dp, borderColor),
    ) {
        Box(
            modifier = Modifier.fillMaxSize().padding(8.dp)
        ) {
            Column(
                modifier = Modifier.fillMaxSize(),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                // Cover Art / Fallback Play Icon (140dp height for prominent showcase)
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(140.dp)
                        .clip(RoundedCornerShape(8.dp))
                        .background(if (coverBitmap != null) Color.Transparent else EmuSyncColors.Surface.copy(alpha = 0.5f)),
                    contentAlignment = Alignment.Center,
                ) {
                    if (coverBitmap != null) {
                        Image(
                            bitmap = coverBitmap,
                            contentDescription = item.name,
                            contentScale = ContentScale.Fit,
                            modifier = Modifier.fillMaxSize(),
                        )
                        if (isHovered) {
                            Box(
                                modifier = Modifier
                                    .fillMaxSize()
                                    .background(Color.Black.copy(alpha = 0.45f)),
                                contentAlignment = Alignment.Center,
                            ) {
                                Icon(
                                    imageVector = Icons.Default.PlayArrow,
                                    contentDescription = "Launch",
                                    tint = Color.White,
                                    modifier = Modifier.size(36.dp),
                                )
                            }
                        }
                    } else {
                        Box(
                            modifier = Modifier
                                .size(56.dp)
                                .clip(RoundedCornerShape(14.dp))
                                .background(EmuSyncColors.PrimaryContainer),
                            contentAlignment = Alignment.Center,
                        ) {
                            Icon(
                                imageVector = Icons.Default.PlayArrow,
                                contentDescription = "Launch",
                                tint = EmuSyncColors.Primary,
                                modifier = Modifier.size(32.dp),
                            )
                        }
                    }
                }

                Spacer(Modifier.height(8.dp))

                // Game name
                Text(
                    text = item.name,
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.SemiBold,
                    color = EmuSyncColors.OnBackground,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.padding(horizontal = 4.dp),
                )
            }

            // Top-right edit button shown only on hover (smooth pure alpha fade, compact 22dp circle)
            if (onEdit != null && editAlpha > 0.01f) {
                Box(
                    modifier = Modifier
                        .align(Alignment.TopEnd)
                        .padding(4.dp)
                        .graphicsLayer { alpha = editAlpha }
                        .size(22.dp)
                        .clip(CircleShape)
                        .background(Color.Black.copy(alpha = 0.55f))
                        .clickable(onClick = { onEdit() }),
                    contentAlignment = Alignment.Center
                ) {
                    Icon(
                        imageVector = Icons.Default.Edit,
                        contentDescription = "Edit Saves",
                        tint = Color.White.copy(alpha = 0.9f),
                        modifier = Modifier.size(12.dp)
                    )
                }
            }
        }
    }
}
