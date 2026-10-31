package com.emusync.ui.components

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Cloud
import androidx.compose.material.icons.filled.CloudSync
import androidx.compose.material.icons.filled.RestartAlt
import androidx.compose.material.icons.filled.SystemUpdate
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.emusync.AppInfo
import com.emusync.ui.AppUiState
import com.emusync.ui.UpdateUiState
import com.emusync.ui.theme.EmuSyncColors

/** Displays connection, update and application actions without owning their state. */
@Composable
internal fun AppHeader(
    state: AppUiState,
    onShowDriveSetup: () -> Unit,
    onConnectDrive: () -> Unit,
    onShowUpdateDialog: () -> Unit,
    onExit: () -> Unit,
) {
    // ── Header ──────────────────────────────────────────
    Box(modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 14.dp)) {
        Text(
            text = "EmuSync",
            style = MaterialTheme.typography.titleLarge,
            fontWeight = FontWeight.Bold,
            color = EmuSyncColors.Primary,
            letterSpacing = 0.5.sp,
            modifier = Modifier.align(Alignment.CenterStart),
        )
        Row(
            modifier = Modifier.align(Alignment.CenterEnd),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // Google Drive status indicator / Connect action
            val googleDrive = state.config?.googleDrive
            val isLinked = googleDrive != null && !googleDrive.refreshToken.isNullOrBlank()
            if (isLinked) {
                Surface(
                    onClick = { onShowDriveSetup() },
                    shape = RoundedCornerShape(8.dp),
                    color = EmuSyncColors.SurfaceVariant,
                    modifier = Modifier.padding(end = 12.dp),
                ) {
                    Row(
                        modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(
                            imageVector = Icons.Default.Cloud,
                            contentDescription = "Drive Connected",
                            tint = EmuSyncColors.Success,
                            modifier = Modifier.size(16.dp),
                        )
                        Spacer(Modifier.width(6.dp))
                        Text(
                            text = "Drive Linked",
                            style = MaterialTheme.typography.labelSmall,
                            color = EmuSyncColors.OnSurface,
                        )
                    }
                }
            } else {
                OutlinedButton(
                    onClick = {
                        if (
                            googleDrive?.clientId.isNullOrBlank() ||
                                googleDrive.clientSecret.isBlank()
                        ) {
                            onShowDriveSetup()
                        } else {
                            onConnectDrive()
                        }
                    },
                    contentPadding = PaddingValues(horizontal = 12.dp, vertical = 6.dp),
                    modifier = Modifier.padding(end = 12.dp),
                ) {
                    Icon(
                        imageVector = Icons.Default.CloudSync,
                        contentDescription = "Connect Google Drive",
                        tint = EmuSyncColors.Primary,
                        modifier = Modifier.size(16.dp),
                    )
                    Spacer(Modifier.width(6.dp))
                    Text("Connect Drive", style = MaterialTheme.typography.labelSmall)
                }
            }

            // Update notification chip
            val updateState = state.updateState
            if (updateState is UpdateUiState.Available) {
                Surface(
                    onClick = { onShowUpdateDialog() },
                    shape = RoundedCornerShape(8.dp),
                    color = EmuSyncColors.Primary,
                    modifier = Modifier.padding(end = 12.dp),
                ) {
                    Row(
                        modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(
                            imageVector = Icons.Default.SystemUpdate,
                            contentDescription = null,
                            tint = Color.White,
                            modifier = Modifier.size(16.dp),
                        )
                        Spacer(Modifier.width(6.dp))
                        Text(
                            text = "Update v${updateState.info.version}",
                            style = MaterialTheme.typography.labelSmall,
                            color = Color.White,
                            fontWeight = FontWeight.Bold,
                        )
                    }
                }
            } else if (updateState is UpdateUiState.ReadyToRestart) {
                Surface(
                    onClick = { onShowUpdateDialog() },
                    shape = RoundedCornerShape(8.dp),
                    color = EmuSyncColors.Success,
                    modifier = Modifier.padding(end = 12.dp),
                ) {
                    Row(
                        modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(
                            imageVector = Icons.Default.RestartAlt,
                            contentDescription = null,
                            tint = Color.White,
                            modifier = Modifier.size(16.dp),
                        )
                        Spacer(Modifier.width(6.dp))
                        Text(
                            text = "Restart EmuSync",
                            style = MaterialTheme.typography.labelSmall,
                            color = Color.White,
                            fontWeight = FontWeight.Bold,
                        )
                    }
                }
            }

            Text(
                text = "v${AppInfo.VERSION}",
                style = MaterialTheme.typography.labelSmall,
                color = EmuSyncColors.OnSurfaceDim,
                modifier = Modifier.padding(end = 16.dp),
            )
            Button(
                onClick = onExit,
                colors =
                    ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error),
                contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
            ) {
                Text("Safe Close")
            }
        }
    }
}
