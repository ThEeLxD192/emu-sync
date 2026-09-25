package com.emusync.ui.dialogs

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
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
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Image
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import com.emusync.ui.GameItem
import com.emusync.ui.components.FilePicker
import com.emusync.ui.components.PickerType
import com.emusync.ui.components.loadGameCover
import com.emusync.ui.theme.EmuSyncColors
import java.io.File

/**
 * A dialog to edit custom cover art and specific save paths for a single game.
 */
@Composable
fun EditGameDialog(
    game: GameItem,
    onDismiss: () -> Unit,
    onSave: (savePaths: List<String>, coverPath: String?) -> Unit,
) {
    var savePaths by remember {
        mutableStateOf(
            if (game.effectiveSavePaths.isEmpty()) listOf("")
            else game.effectiveSavePaths
        )
    }

    var coverPath by remember {
        mutableStateOf(game.effectiveCoverPath ?: "")
    }

    val previewFile = remember(coverPath) {
        val trimmed = coverPath.trim()
        if (trimmed.isNotBlank()) {
            File(trimmed).takeIf { it.exists() && it.isFile }
        } else {
            game.coverFile
        }
    }

    val previewBitmap = remember(previewFile?.absolutePath) {
        loadGameCover(previewFile)
    }

    val isCustomCover = coverPath.isNotBlank()
    val isAutoDetected = !isCustomCover && previewFile != null

    Dialog(onDismissRequest = onDismiss) {
        Surface(
            shape = RoundedCornerShape(16.dp),
            color = EmuSyncColors.Surface,
            border = BorderStroke(1.dp, EmuSyncColors.Divider),
            modifier = Modifier
                .widthIn(min = 520.dp, max = 580.dp)
                .wrapContentHeight(),
        ) {
            Column(
                modifier = Modifier
                    .padding(24.dp)
                    .heightIn(max = 640.dp)
                    .verticalScroll(rememberScrollState())
            ) {
                Text(
                    text = "Edit Game",
                    style = MaterialTheme.typography.headlineSmall,
                    fontWeight = FontWeight.Bold,
                    color = EmuSyncColors.OnSurface,
                )
                Spacer(Modifier.height(4.dp))
                Text(
                    text = game.name,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                    color = EmuSyncColors.Primary,
                )
                Text(
                    text = "Configure custom cover art and specific save files/folders.",
                    style = MaterialTheme.typography.bodySmall,
                    color = EmuSyncColors.OnSurfaceDim,
                )
                Spacer(Modifier.height(18.dp))

                // ── Cover Art Section ─────────────────────────────
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        text = "Cover Art",
                        style = MaterialTheme.typography.titleSmall,
                        fontWeight = FontWeight.SemiBold,
                        color = EmuSyncColors.OnSurface,
                    )
                    // Status Badge
                    val badgeText = when {
                        isCustomCover -> "Custom Cover"
                        isAutoDetected -> "Auto-detected"
                        else -> "No Cover"
                    }
                    val badgeColor = when {
                        isCustomCover -> EmuSyncColors.Primary
                        isAutoDetected -> EmuSyncColors.Success
                        else -> EmuSyncColors.OnSurfaceDim
                    }
                    Box(
                        modifier = Modifier
                            .clip(RoundedCornerShape(6.dp))
                            .background(badgeColor.copy(alpha = 0.15f))
                            .padding(horizontal = 8.dp, vertical = 2.dp)
                    ) {
                        Text(
                            text = badgeText,
                            style = MaterialTheme.typography.labelSmall,
                            color = badgeColor,
                            fontWeight = FontWeight.Medium,
                        )
                    }
                }
                Spacer(Modifier.height(4.dp))
                Text(
                    text = "Individual cover image for this game, or empty for automatic detection.",
                    style = MaterialTheme.typography.bodySmall,
                    color = EmuSyncColors.OnSurfaceDim,
                )
                Spacer(Modifier.height(12.dp))

                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    // Generous Box Art Preview (136 x 180 dp)
                    Box(
                        modifier = Modifier
                            .size(136.dp, 180.dp)
                            .clip(RoundedCornerShape(10.dp))
                            .background(EmuSyncColors.SurfaceVariant)
                            .border(BorderStroke(1.dp, EmuSyncColors.Divider), RoundedCornerShape(10.dp)),
                        contentAlignment = Alignment.Center,
                    ) {
                        if (previewBitmap != null) {
                            Image(
                                bitmap = previewBitmap,
                                contentDescription = "Cover preview",
                                contentScale = ContentScale.Fit,
                                modifier = Modifier
                                    .fillMaxSize()
                                    .padding(4.dp),
                            )
                        } else {
                            Column(
                                horizontalAlignment = Alignment.CenterHorizontally,
                                verticalArrangement = Arrangement.Center,
                                modifier = Modifier.padding(8.dp),
                            ) {
                                Icon(
                                    imageVector = Icons.Default.Image,
                                    contentDescription = null,
                                    tint = EmuSyncColors.OnSurfaceDim.copy(alpha = 0.35f),
                                    modifier = Modifier.size(42.dp),
                                )
                                Spacer(Modifier.height(6.dp))
                                Text(
                                    text = "No Cover Found",
                                    style = MaterialTheme.typography.labelSmall,
                                    color = EmuSyncColors.OnSurfaceDim.copy(alpha = 0.6f),
                                )
                            }
                        }
                    }

                    Spacer(Modifier.width(16.dp))

                    Column(
                        modifier = Modifier.weight(1f),
                        verticalArrangement = Arrangement.spacedBy(10.dp)
                    ) {
                        Text(
                            text = if (isCustomCover) "Custom Image File:" else "Detected Image File:",
                            style = MaterialTheme.typography.labelMedium,
                            color = EmuSyncColors.OnSurfaceDim,
                        )

                        OutlinedTextField(
                            value = coverPath,
                            onValueChange = { coverPath = it },
                            modifier = Modifier.fillMaxWidth(),
                            singleLine = true,
                            placeholder = {
                                Text(
                                    text = if (game.coverFile != null) game.coverFile?.name.orEmpty() else "Auto-detecting...",
                                    color = EmuSyncColors.OnSurfaceDim.copy(alpha = 0.5f),
                                    maxLines = 1,
                                )
                            },
                            colors = OutlinedTextFieldDefaults.colors(
                                focusedBorderColor = EmuSyncColors.Primary,
                                unfocusedBorderColor = EmuSyncColors.Divider,
                                cursorColor = EmuSyncColors.Primary,
                                focusedTextColor = EmuSyncColors.OnBackground,
                                unfocusedTextColor = EmuSyncColors.OnSurface,
                            ),
                            shape = RoundedCornerShape(10.dp),
                        )

                        Text(
                            text = when {
                                isCustomCover -> "Using your specific custom image for this game."
                                isAutoDetected -> "Found automatically next to ROM or emulator covers."
                                else -> "Select an image file to display as box art."
                            },
                            style = MaterialTheme.typography.bodySmall,
                            color = EmuSyncColors.OnSurfaceDim.copy(alpha = 0.75f),
                        )

                        Row(
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Button(
                                onClick = {
                                    val initialDir = coverPath.ifBlank {
                                        game.romFile?.parentFile?.absolutePath ?: ""
                                    }
                                    val result = FilePicker.pickPath(
                                        label = "Cover Image",
                                        initialPath = initialDir,
                                        type = PickerType.FILE
                                    )
                                    if (result != null) coverPath = result
                                },
                                colors = ButtonDefaults.buttonColors(
                                    containerColor = EmuSyncColors.SurfaceSelected,
                                    contentColor = EmuSyncColors.OnSurface,
                                ),
                                shape = RoundedCornerShape(8.dp),
                            ) {
                                Text(if (isCustomCover) "Change Image..." else "Select Image...")
                            }
                            if (isCustomCover) {
                                TextButton(onClick = { coverPath = "" }) {
                                    Text("Reset to Auto", color = EmuSyncColors.Primary)
                                }
                            }
                        }
                    }
                }

                Spacer(Modifier.height(22.dp))

                // ── Save Paths Section ────────────────────────────
                Text(
                    text = "Save Files/Directories Paths",
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.SemiBold,
                    color = EmuSyncColors.OnSurface,
                )
                Spacer(Modifier.height(4.dp))
                Text(
                    text = "Specific save files or directories synchronized for this game.",
                    style = MaterialTheme.typography.bodySmall,
                    color = EmuSyncColors.OnSurfaceDim,
                )
                Spacer(Modifier.height(10.dp))

                savePaths.forEachIndexed { index, path ->
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(bottom = 8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Box(modifier = Modifier.weight(1f)) {
                            EditPathField(
                                value = path,
                                onValueChange = { newVal ->
                                    val newList = savePaths.toMutableList()
                                    newList[index] = newVal
                                    savePaths = newList
                                },
                                placeholder = "/home/user/saves/game.sav",
                            )
                        }
                        if (savePaths.size > 1) {
                            Spacer(Modifier.width(8.dp))
                            IconButton(onClick = {
                                val newList = savePaths.toMutableList()
                                newList.removeAt(index)
                                savePaths = newList
                            }) {
                                Icon(
                                    Icons.Default.Delete,
                                    contentDescription = "Remove Path",
                                    tint = EmuSyncColors.Error
                                )
                            }
                        }
                    }
                }

                TextButton(
                    onClick = { savePaths = savePaths + "" },
                    modifier = Modifier.align(Alignment.Start)
                ) {
                    Text("+ Add Another Path", color = EmuSyncColors.Primary)
                }

                Spacer(Modifier.height(20.dp))

                // ── Action Buttons ────────────────────────────────
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
                ) {
                    TextButton(onClick = onDismiss) {
                        Text("Cancel")
                    }
                    Button(
                        onClick = {
                            val validSavePaths = savePaths.map { it.trim() }.filter { it.isNotBlank() }
                            val cleanCover = coverPath.trim().takeIf { it.isNotBlank() }
                            onSave(validSavePaths, cleanCover)
                        },
                        colors = ButtonDefaults.buttonColors(containerColor = EmuSyncColors.Primary),
                    ) {
                        Text("Save")
                    }
                }
            }
        }
    }
}

@Composable
private fun EditPathField(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        OutlinedTextField(
            value = value,
            onValueChange = onValueChange,
            modifier = Modifier.weight(1f),
            singleLine = true,
            placeholder = {
                Text(
                    text = placeholder,
                    color = EmuSyncColors.OnSurfaceDim.copy(alpha = 0.5f),
                )
            },
            colors = OutlinedTextFieldDefaults.colors(
                focusedBorderColor = EmuSyncColors.Primary,
                unfocusedBorderColor = EmuSyncColors.Divider,
                cursorColor = EmuSyncColors.Primary,
                focusedTextColor = EmuSyncColors.OnBackground,
                unfocusedTextColor = EmuSyncColors.OnSurface,
            ),
            shape = RoundedCornerShape(10.dp),
        )
        Spacer(Modifier.width(8.dp))
        Button(
            onClick = {
                val result = FilePicker.pickPath(
                    label = "File",
                    initialPath = value,
                    type = PickerType.FILE
                )
                if (result != null) onValueChange(result)
            },
            colors = ButtonDefaults.buttonColors(
                containerColor = EmuSyncColors.SurfaceSelected,
                contentColor = EmuSyncColors.OnSurface,
            ),
            shape = RoundedCornerShape(10.dp),
            modifier = Modifier.height(56.dp),
        ) {
            Text("File")
        }
        Spacer(Modifier.width(8.dp))
        Button(
            onClick = {
                val result = FilePicker.pickPath(
                    label = "Folder",
                    initialPath = value,
                    type = PickerType.DIRECTORY
                )
                if (result != null) onValueChange(result)
            },
            colors = ButtonDefaults.buttonColors(
                containerColor = EmuSyncColors.SurfaceSelected,
                contentColor = EmuSyncColors.OnSurface,
            ),
            shape = RoundedCornerShape(10.dp),
            modifier = Modifier.height(56.dp),
        ) {
            Text("Folder")
        }
    }
}
