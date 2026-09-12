package com.emusync.ui.dialogs

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Cloud
import androidx.compose.material.icons.filled.CloudSync
import androidx.compose.material.icons.filled.Computer
import androidx.compose.material.icons.filled.ErrorOutline
import androidx.compose.material.icons.filled.HourglassTop
import androidx.compose.material.icons.filled.SportsEsports
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import com.emusync.drive.SyncDecision
import com.emusync.ui.AppStatus
import com.emusync.ui.theme.EmuSyncColors

/**
 * Full-screen overlay showing the current sync/game status.
 * Displayed during sync, play, and upload phases.
 */
@Composable
fun StatusOverlay(
    status: AppStatus,
    onDismissError: (() -> Unit)? = null,
) {
    when (status) {
        is AppStatus.Syncing -> StatusDialog(
            icon = Icons.Default.CloudSync,
            title = status.message,
            spinning = true,
            color = EmuSyncColors.Primary,
        )
        is AppStatus.Playing -> StatusDialog(
            icon = Icons.Default.SportsEsports,
            title = "Playing...",
            subtitle = "The game is running. This window will update when you close it.",
            spinning = false,
            color = EmuSyncColors.Success,
        )
        is AppStatus.Loading -> StatusDialog(
            icon = Icons.Default.HourglassTop,
            title = "Loading...",
            spinning = true,
            color = EmuSyncColors.OnSurfaceDim,
        )
        is AppStatus.Error -> ErrorDialog(
            message = status.message,
            onDismiss = { onDismissError?.invoke() }
        )
        is AppStatus.Conflict -> ConflictDialog(
            gameName = status.gameName,
            localDate = status.localDate,
            cloudDate = status.cloudDate,
            onResolve = status.onResolve,
        )
        is AppStatus.Idle -> { /* no overlay */ }
    }
}

@Composable
private fun StatusDialog(
    icon: ImageVector,
    title: String,
    subtitle: String? = null,
    spinning: Boolean,
    color: Color,
) {
    // Spinner animation
    val infiniteTransition = rememberInfiniteTransition()
    val rotation by infiniteTransition.animateFloat(
        initialValue = 0f,
        targetValue = 360f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 1200, easing = LinearEasing),
        ),
    )

    Dialog(onDismissRequest = {}) {
        Surface(
            shape = RoundedCornerShape(20.dp),
            color = EmuSyncColors.Surface,
            border = BorderStroke(1.dp, EmuSyncColors.Divider),
            tonalElevation = 8.dp,
        ) {
            Column(
                modifier = Modifier.padding(32.dp).widthIn(min = 280.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Box(
                    modifier = Modifier
                        .size(64.dp)
                        .clip(CircleShape)
                        .background(color.copy(alpha = 0.15f)),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(
                        imageVector = icon,
                        contentDescription = null,
                        tint = color,
                        modifier = Modifier
                            .size(32.dp)
                            .then(if (spinning) Modifier.rotate(rotation) else Modifier),
                    )
                }

                Spacer(Modifier.height(20.dp))

                Text(
                    text = title,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                    color = EmuSyncColors.OnBackground,
                    textAlign = TextAlign.Center,
                )

                if (subtitle != null) {
                    Spacer(Modifier.height(8.dp))
                    Text(
                        text = subtitle,
                        style = MaterialTheme.typography.bodySmall,
                        color = EmuSyncColors.OnSurfaceDim,
                        textAlign = TextAlign.Center,
                    )
                }
            }
        }
    }
}

@Composable
private fun ErrorDialog(
    message: String,
    onDismiss: () -> Unit,
) {
    Dialog(onDismissRequest = onDismiss) {
        Surface(
            shape = RoundedCornerShape(20.dp),
            color = EmuSyncColors.Surface,
            border = BorderStroke(1.dp, EmuSyncColors.Error.copy(alpha = 0.5f)),
        ) {
            Column(
                modifier = Modifier.padding(28.dp).widthIn(min = 300.dp, max = 420.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Icon(
                    Icons.Default.ErrorOutline,
                    contentDescription = null,
                    tint = EmuSyncColors.Error,
                    modifier = Modifier.size(48.dp),
                )
                Spacer(Modifier.height(16.dp))
                Text(
                    text = "Sync Notice",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Bold,
                    color = EmuSyncColors.Error,
                )
                Spacer(Modifier.height(8.dp))
                Text(
                    text = message,
                    style = MaterialTheme.typography.bodyMedium,
                    color = EmuSyncColors.OnSurface,
                    textAlign = TextAlign.Center,
                )
                Spacer(Modifier.height(20.dp))
                Button(
                    onClick = onDismiss,
                    colors = ButtonDefaults.buttonColors(containerColor = EmuSyncColors.Primary),
                    shape = RoundedCornerShape(10.dp),
                ) {
                    Text("Close")
                }
            }
        }
    }
}

@Composable
private fun ConflictDialog(
    gameName: String,
    localDate: String,
    cloudDate: String,
    onResolve: (SyncDecision) -> Unit,
) {
    var resolved by remember(gameName, localDate, cloudDate) { mutableStateOf(false) }

    val infiniteTransition = rememberInfiniteTransition()
    val rotation by infiniteTransition.animateFloat(
        initialValue = 0f,
        targetValue = 360f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 1200, easing = LinearEasing),
        ),
    )

    Dialog(onDismissRequest = {}) {
        Surface(
            shape = RoundedCornerShape(20.dp),
            color = EmuSyncColors.Surface,
            border = BorderStroke(
                1.dp,
                if (resolved) EmuSyncColors.Primary.copy(alpha = 0.5f)
                else EmuSyncColors.Warning.copy(alpha = 0.5f),
            ),
        ) {
            if (resolved) {
                Column(
                    modifier = Modifier.padding(32.dp).widthIn(min = 280.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Box(
                        modifier = Modifier
                            .size(64.dp)
                            .clip(CircleShape)
                            .background(EmuSyncColors.Primary.copy(alpha = 0.15f)),
                        contentAlignment = Alignment.Center,
                    ) {
                        Icon(
                            imageVector = Icons.Default.CloudSync,
                            contentDescription = null,
                            tint = EmuSyncColors.Primary,
                            modifier = Modifier.size(32.dp).rotate(rotation),
                        )
                    }

                    Spacer(Modifier.height(20.dp))

                    Text(
                        text = "Syncing...",
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold,
                        color = EmuSyncColors.OnBackground,
                        textAlign = TextAlign.Center,
                    )
                }
            } else {
                Column(
                    modifier = Modifier.padding(28.dp).widthIn(min = 320.dp, max = 440.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Icon(
                        Icons.Default.Warning,
                        contentDescription = null,
                        tint = EmuSyncColors.Warning,
                        modifier = Modifier.size(48.dp),
                    )

                    Spacer(Modifier.height(16.dp))

                    Text(
                        text = "Save Conflict",
                        style = MaterialTheme.typography.titleLarge,
                        fontWeight = FontWeight.Bold,
                        color = EmuSyncColors.Warning,
                    )

                    Spacer(Modifier.height(4.dp))

                    Text(
                        text = gameName,
                        style = MaterialTheme.typography.bodyMedium,
                        color = EmuSyncColors.OnSurfaceDim,
                    )

                    Spacer(Modifier.height(20.dp))

                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        ConflictOptionCard(
                            title = "Local Save",
                            date = localDate,
                            icon = Icons.Default.Computer,
                            color = EmuSyncColors.Primary,
                            onClick = {
                                resolved = true
                                onResolve(SyncDecision.UPLOAD_LOCAL)
                            },
                            modifier = Modifier.weight(1f),
                        )

                        ConflictOptionCard(
                            title = "Cloud Save",
                            date = cloudDate,
                            icon = Icons.Default.Cloud,
                            color = EmuSyncColors.Secondary,
                            onClick = {
                                resolved = true
                                onResolve(SyncDecision.DOWNLOAD_CLOUD)
                            },
                            modifier = Modifier.weight(1f),
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun ConflictOptionCard(
    title: String,
    date: String,
    icon: ImageVector,
    color: Color,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    OutlinedButton(
        onClick = onClick,
        modifier = modifier.height(IntrinsicSize.Min),
        shape = RoundedCornerShape(12.dp),
        border = BorderStroke(1.dp, color.copy(alpha = 0.4f)),
        colors = ButtonDefaults.outlinedButtonColors(
            containerColor = color.copy(alpha = 0.08f),
        ),
    ) {
        Column(
            modifier = Modifier.padding(vertical = 12.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Icon(icon, contentDescription = null, tint = color, modifier = Modifier.size(28.dp))
            Spacer(Modifier.height(8.dp))
            Text(
                text = title,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.SemiBold,
                color = color,
            )
            Spacer(Modifier.height(4.dp))
            Text(
                text = date,
                style = MaterialTheme.typography.labelSmall,
                color = EmuSyncColors.OnSurfaceDim,
                textAlign = TextAlign.Center,
            )
        }
    }
}
