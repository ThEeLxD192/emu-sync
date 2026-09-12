package com.emusync.ui.dialogs

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.RestartAlt
import androidx.compose.material.icons.filled.SystemUpdate
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import com.emusync.AppInfo
import com.emusync.ui.UpdateUiState
import com.emusync.ui.theme.EmuSyncColors
import com.emusync.update.UpdateInfo
import java.io.File

@Composable
fun UpdateDialog(
    state: UpdateUiState,
    onDismiss: () -> Unit,
    onStartDownload: (UpdateInfo) -> Unit,
    onRestart: (File) -> Unit,
) {
    Dialog(onDismissRequest = {
        if (state !is UpdateUiState.Downloading) {
            onDismiss()
        }
    }) {
        Surface(
            shape = RoundedCornerShape(16.dp),
            color = EmuSyncColors.Surface,
            modifier = Modifier
                .widthIn(min = 400.dp, max = 500.dp)
                .wrapContentHeight(),
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(24.dp)
            ) {
                when (state) {
                    is UpdateUiState.Available -> {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Box(
                                modifier = Modifier
                                    .size(40.dp)
                                    .clip(RoundedCornerShape(10.dp))
                                    .background(EmuSyncColors.PrimaryContainer),
                                contentAlignment = Alignment.Center,
                            ) {
                                Icon(
                                    imageVector = Icons.Default.SystemUpdate,
                                    contentDescription = null,
                                    tint = EmuSyncColors.Primary,
                                )
                            }
                            Spacer(Modifier.width(12.dp))
                            Column {
                                Text(
                                    text = "Update Available",
                                    style = MaterialTheme.typography.titleLarge,
                                    fontWeight = FontWeight.Bold,
                                    color = EmuSyncColors.OnSurface,
                                )
                                Text(
                                    text = "Version ${state.info.version} (current: ${AppInfo.VERSION})",
                                    style = MaterialTheme.typography.bodySmall,
                                    color = EmuSyncColors.OnSurfaceDim,
                                )
                            }
                        }

                        Spacer(Modifier.height(16.dp))

                        Text(
                            text = "Release Notes:",
                            style = MaterialTheme.typography.labelMedium,
                            fontWeight = FontWeight.SemiBold,
                            color = EmuSyncColors.OnSurface,
                        )
                        Spacer(Modifier.height(6.dp))

                        Box(
                            modifier = Modifier
                                .fillMaxWidth()
                                .heightIn(max = 160.dp)
                                .clip(RoundedCornerShape(8.dp))
                                .background(EmuSyncColors.SurfaceVariant)
                                .padding(12.dp)
                                .verticalScroll(rememberScrollState())
                        ) {
                            Text(
                                text = state.info.releaseNotes,
                                style = MaterialTheme.typography.bodySmall,
                                color = EmuSyncColors.OnSurface,
                            )
                        }

                        Spacer(Modifier.height(20.dp))

                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
                        ) {
                            TextButton(onClick = onDismiss) {
                                Text("Later")
                            }
                            Button(
                                onClick = { onStartDownload(state.info) },
                                colors = ButtonDefaults.buttonColors(containerColor = EmuSyncColors.Primary),
                            ) {
                                Icon(Icons.Default.Download, contentDescription = null, modifier = Modifier.size(18.dp))
                                Spacer(Modifier.width(6.dp))
                                Text("Update Now")
                            }
                        }
                    }

                    is UpdateUiState.Downloading -> {
                        Text(
                            text = "Downloading update...",
                            style = MaterialTheme.typography.titleMedium,
                            fontWeight = FontWeight.Bold,
                            color = EmuSyncColors.OnSurface,
                        )
                        Spacer(Modifier.height(6.dp))
                        Text(
                            text = "Downloading version ${state.info.version} from GitHub",
                            style = MaterialTheme.typography.bodySmall,
                            color = EmuSyncColors.OnSurfaceDim,
                        )

                        Spacer(Modifier.height(16.dp))
                        LinearProgressIndicator(
                            progress = { state.progress },
                            modifier = Modifier.fillMaxWidth().height(8.dp).clip(RoundedCornerShape(4.dp)),
                            color = EmuSyncColors.Primary,
                        )
                        Spacer(Modifier.height(8.dp))
                        Text(
                            text = "${(state.progress * 100).toInt()}%",
                            style = MaterialTheme.typography.labelSmall,
                            color = EmuSyncColors.OnSurfaceDim,
                            modifier = Modifier.align(Alignment.End),
                        )
                    }

                    is UpdateUiState.ReadyToRestart -> {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Box(
                                modifier = Modifier
                                    .size(40.dp)
                                    .clip(RoundedCornerShape(10.dp))
                                    .background(EmuSyncColors.PrimaryContainer),
                                contentAlignment = Alignment.Center,
                            ) {
                                Icon(
                                    imageVector = Icons.Default.RestartAlt,
                                    contentDescription = null,
                                    tint = EmuSyncColors.Primary,
                                )
                            }
                            Spacer(Modifier.width(12.dp))
                            Column {
                                Text(
                                    text = "Update Complete!",
                                    style = MaterialTheme.typography.titleMedium,
                                    fontWeight = FontWeight.Bold,
                                    color = EmuSyncColors.OnSurface,
                                )
                                Text(
                                    text = "The new AppImage is ready.",
                                    style = MaterialTheme.typography.bodySmall,
                                    color = EmuSyncColors.OnSurfaceDim,
                                )
                            }
                        }

                        Spacer(Modifier.height(16.dp))
                        Text(
                            text = "To apply changes and enjoy the new version, restart EmuSync.",
                            style = MaterialTheme.typography.bodyMedium,
                            color = EmuSyncColors.OnSurface,
                        )

                        Spacer(Modifier.height(20.dp))
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
                        ) {
                            TextButton(onClick = onDismiss) {
                                Text("Restart Later")
                            }
                            Button(
                                onClick = { onRestart(state.downloadedFile) },
                                colors = ButtonDefaults.buttonColors(containerColor = EmuSyncColors.Primary),
                            ) {
                                Text("Restart Now")
                            }
                        }
                    }

                    is UpdateUiState.Error -> {
                        Text(
                            text = "Update Error",
                            style = MaterialTheme.typography.titleMedium,
                            fontWeight = FontWeight.Bold,
                            color = EmuSyncColors.Error,
                        )
                        Spacer(Modifier.height(8.dp))
                        Text(
                            text = state.message,
                            style = MaterialTheme.typography.bodyMedium,
                            color = EmuSyncColors.OnSurface,
                        )
                        Spacer(Modifier.height(16.dp))
                        Button(
                            onClick = onDismiss,
                            modifier = Modifier.align(Alignment.End),
                            colors = ButtonDefaults.buttonColors(containerColor = EmuSyncColors.Primary),
                        ) {
                            Text("Close")
                        }
                    }

                    else -> {}
                }
            }
        }
    }
}
