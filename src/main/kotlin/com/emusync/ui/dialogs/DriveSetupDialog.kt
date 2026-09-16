package com.emusync.ui.dialogs

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Cloud
import androidx.compose.material.icons.filled.CloudSync
import androidx.compose.material.icons.filled.FolderOpen
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.LinkOff
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import com.emusync.model.GoogleDriveConfig
import com.emusync.ui.theme.EmuSyncColors
import java.awt.Desktop
import java.io.File

/**
 * Dialog for viewing and configuring Google Drive OAuth credentials,
 * unlinking an account, or opening the directory containing config.json.
 */
@Composable
fun DriveSetupDialog(
    initialConfig: GoogleDriveConfig?,
    configFile: File,
    onDismiss: () -> Unit,
    onSaveAndConnect: (clientId: String, clientSecret: String) -> Unit,
    onUnlink: () -> Unit,
) {
    var clientId by remember { mutableStateOf(initialConfig?.clientId ?: "") }
    var clientSecret by remember { mutableStateOf(initialConfig?.clientSecret ?: "") }
    var errorMessage by remember { mutableStateOf<String?>(null) }
    val isLinked = !initialConfig?.refreshToken.isNullOrBlank()

    Dialog(onDismissRequest = onDismiss) {
        Surface(
            shape = RoundedCornerShape(16.dp),
            color = EmuSyncColors.Surface,
            border = BorderStroke(1.dp, EmuSyncColors.Divider),
            modifier = Modifier
                .widthIn(min = 450.dp, max = 560.dp)
                .wrapContentHeight(),
        ) {
            Column(
                modifier = Modifier
                    .padding(24.dp)
                    .verticalScroll(rememberScrollState()),
            ) {
                // Header
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Icon(
                        imageVector = if (isLinked) Icons.Default.Cloud else Icons.Default.CloudSync,
                        contentDescription = null,
                        tint = if (isLinked) EmuSyncColors.Success else EmuSyncColors.Primary,
                        modifier = Modifier.size(28.dp),
                    )
                    Spacer(Modifier.width(12.dp))
                    Column {
                        Text(
                            text = "Google Drive Sync",
                            style = MaterialTheme.typography.titleLarge,
                            fontWeight = FontWeight.Bold,
                            color = EmuSyncColors.OnSurface,
                        )
                        Text(
                            text = if (isLinked) "Account linked and active" else "Configure cross-device save synchronization",
                            style = MaterialTheme.typography.bodySmall,
                            color = if (isLinked) EmuSyncColors.Success else EmuSyncColors.OnSurfaceDim,
                        )
                    }
                }

                Spacer(Modifier.height(16.dp))

                // Info banner
                Surface(
                    shape = RoundedCornerShape(8.dp),
                    color = EmuSyncColors.SurfaceVariant,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Row(
                        modifier = Modifier.padding(12.dp),
                        verticalAlignment = Alignment.Top,
                    ) {
                        Icon(
                            imageVector = Icons.Default.Info,
                            contentDescription = null,
                            tint = EmuSyncColors.Primary,
                            modifier = Modifier.size(18.dp).padding(top = 2.dp),
                        )
                        Spacer(Modifier.width(10.dp))
                        Text(
                            text = "Requires an OAuth 2.0 Client ID (Desktop App) from Google Cloud Console with the Google Drive API enabled.",
                            style = MaterialTheme.typography.bodySmall,
                            color = EmuSyncColors.OnSurfaceDim,
                            lineHeight = 18.sp,
                        )
                    }
                }

                Spacer(Modifier.height(16.dp))

                // Client ID Field
                OutlinedTextField(
                    value = clientId,
                    onValueChange = {
                        clientId = it
                        errorMessage = null
                    },
                    label = { Text("Client ID") },
                    placeholder = { Text("xxxxxx.apps.googleusercontent.com") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedBorderColor = EmuSyncColors.Primary,
                        unfocusedBorderColor = EmuSyncColors.Divider,
                        focusedTextColor = EmuSyncColors.OnSurface,
                        unfocusedTextColor = EmuSyncColors.OnSurface,
                    ),
                )

                Spacer(Modifier.height(12.dp))

                // Client Secret Field
                OutlinedTextField(
                    value = clientSecret,
                    onValueChange = {
                        clientSecret = it
                        errorMessage = null
                    },
                    label = { Text("Client Secret") },
                    placeholder = { Text("GOCSPX-xxxxxx") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedBorderColor = EmuSyncColors.Primary,
                        unfocusedBorderColor = EmuSyncColors.Divider,
                        focusedTextColor = EmuSyncColors.OnSurface,
                        unfocusedTextColor = EmuSyncColors.OnSurface,
                    ),
                )

                if (errorMessage != null) {
                    Spacer(Modifier.height(8.dp))
                    Text(
                        text = errorMessage ?: "",
                        color = EmuSyncColors.Error,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }

                Spacer(Modifier.height(16.dp))

                // Config file location & Open Folder button
                Surface(
                    shape = RoundedCornerShape(8.dp),
                    color = EmuSyncColors.Background,
                    border = BorderStroke(1.dp, EmuSyncColors.Divider),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Row(
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Column(modifier = Modifier.weight(1f)) {
                            Text(
                                text = "Config Location:",
                                style = MaterialTheme.typography.labelSmall,
                                color = EmuSyncColors.OnSurfaceDim,
                            )
                            Text(
                                text = configFile.absolutePath,
                                style = MaterialTheme.typography.bodySmall,
                                fontFamily = FontFamily.Monospace,
                                color = EmuSyncColors.OnSurface,
                                maxLines = 1,
                            )
                        }
                        Spacer(Modifier.width(8.dp))
                        OutlinedButton(
                            onClick = { openConfigDirectory(configFile) },
                            contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 10.dp, vertical = 4.dp),
                        ) {
                            Icon(
                                imageVector = Icons.Default.FolderOpen,
                                contentDescription = "Open Folder",
                                modifier = Modifier.size(16.dp),
                            )
                            Spacer(Modifier.width(6.dp))
                            Text("Open Folder", style = MaterialTheme.typography.labelSmall)
                        }
                    }
                }

                Spacer(Modifier.height(24.dp))

                // Action buttons
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    // Left action: Unlink if connected
                    if (isLinked) {
                        TextButton(
                            onClick = {
                                onUnlink()
                                onDismiss()
                            },
                            colors = ButtonDefaults.textButtonColors(contentColor = EmuSyncColors.Error),
                        ) {
                            Icon(Icons.Default.LinkOff, contentDescription = null, modifier = Modifier.size(16.dp))
                            Spacer(Modifier.width(6.dp))
                            Text("Unlink Account")
                        }
                    } else {
                        Spacer(Modifier.width(1.dp))
                    }

                    // Right actions: Cancel + Save & Connect
                    Row {
                        TextButton(
                            onClick = onDismiss,
                            colors = ButtonDefaults.textButtonColors(contentColor = EmuSyncColors.OnSurfaceDim),
                        ) {
                            Text("Cancel")
                        }
                        Spacer(Modifier.width(8.dp))
                        Button(
                            onClick = {
                                if (clientId.isBlank() || clientSecret.isBlank()) {
                                    errorMessage = "Please enter both Client ID and Client Secret."
                                } else {
                                    onSaveAndConnect(clientId.trim(), clientSecret.trim())
                                    onDismiss()
                                }
                            },
                            colors = ButtonDefaults.buttonColors(
                                containerColor = EmuSyncColors.Primary,
                                contentColor = androidx.compose.ui.graphics.Color.White,
                            ),
                        ) {
                            Icon(Icons.Default.CloudSync, contentDescription = null, modifier = Modifier.size(16.dp))
                            Spacer(Modifier.width(6.dp))
                            Text("Save & Connect")
                        }
                    }
                }
            }
        }
    }
}

/**
 * Cross-platform helper to reveal the configuration file's parent folder in the OS file explorer.
 */
private fun openConfigDirectory(configFile: File) {
    try {
        val dir = if (configFile.isDirectory) configFile else configFile.parentFile ?: configFile
        dir.mkdirs()
        if (Desktop.isDesktopSupported() && Desktop.getDesktop().isSupported(Desktop.Action.OPEN)) {
            Desktop.getDesktop().open(dir)
        } else {
            val os = System.getProperty("os.name", "").lowercase()
            when {
                os.contains("windows") -> ProcessBuilder("explorer", dir.absolutePath).start()
                os.contains("mac") -> ProcessBuilder("open", dir.absolutePath).start()
                else -> ProcessBuilder("xdg-open", dir.absolutePath).start()
            }
        }
    } catch (_: Exception) {
        // Ignored if headless or unsupported
    }
}
