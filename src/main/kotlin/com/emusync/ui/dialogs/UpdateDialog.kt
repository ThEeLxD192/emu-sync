package com.emusync.ui.dialogs

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.RestartAlt
import androidx.compose.material.icons.filled.SystemUpdate
import androidx.compose.material3.*
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
import java.io.File

@Composable
fun UpdateDialog(
    state: UpdateUiState,
    onDismiss: () -> Unit,
    onStartDownload: (com.emusync.update.UpdateInfo) -> Unit,
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
                                    text = "Actualización disponible",
                                    style = MaterialTheme.typography.titleLarge,
                                    fontWeight = FontWeight.Bold,
                                    color = EmuSyncColors.OnSurface,
                                )
                                Text(
                                    text = "Versión ${state.info.version} (actual: ${AppInfo.VERSION})",
                                    style = MaterialTheme.typography.bodySmall,
                                    color = EmuSyncColors.OnSurfaceDim,
                                )
                            }
                        }

                        Spacer(Modifier.height(16.dp))

                        Text(
                            text = "Novedades:",
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
                                Text("Más tarde")
                            }
                            Button(
                                onClick = { onStartDownload(state.info) },
                                colors = ButtonDefaults.buttonColors(containerColor = EmuSyncColors.Primary),
                            ) {
                                Icon(Icons.Default.Download, contentDescription = null, modifier = Modifier.size(18.dp))
                                Spacer(Modifier.width(6.dp))
                                Text("Actualizar ahora")
                            }
                        }
                    }

                    is UpdateUiState.Downloading -> {
                        Text(
                            text = "Descargando actualización...",
                            style = MaterialTheme.typography.titleMedium,
                            fontWeight = FontWeight.Bold,
                            color = EmuSyncColors.OnSurface,
                        )
                        Spacer(Modifier.height(6.dp))
                        Text(
                            text = "Descargando versión ${state.info.version} de GitHub",
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
                                    text = "¡Actualización completada!",
                                    style = MaterialTheme.typography.titleMedium,
                                    fontWeight = FontWeight.Bold,
                                    color = EmuSyncColors.OnSurface,
                                )
                                Text(
                                    text = "El nuevo AppImage está listo.",
                                    style = MaterialTheme.typography.bodySmall,
                                    color = EmuSyncColors.OnSurfaceDim,
                                )
                            }
                        }

                        Spacer(Modifier.height(16.dp))
                        Text(
                            text = "Para aplicar los cambios y disfrutar de la nueva versión, reinicia EmuSync.",
                            style = MaterialTheme.typography.bodyMedium,
                            color = EmuSyncColors.OnSurface,
                        )

                        Spacer(Modifier.height(20.dp))
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
                        ) {
                            TextButton(onClick = onDismiss) {
                                Text("Reiniciar después")
                            }
                            Button(
                                onClick = { onRestart(state.downloadedFile) },
                                colors = ButtonDefaults.buttonColors(containerColor = EmuSyncColors.Primary),
                            ) {
                                Text("Reiniciar ahora")
                            }
                        }
                    }

                    is UpdateUiState.Error -> {
                        Text(
                            text = "Error al actualizar",
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
                            Text("Cerrar")
                        }
                    }

                    else -> {}
                }
            }
        }
    }
}
