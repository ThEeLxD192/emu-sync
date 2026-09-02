package com.emusync.ui.dialogs

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import com.emusync.ui.GameItem
import com.emusync.ui.components.FilePicker
import com.emusync.ui.components.PickerType
import com.emusync.ui.theme.EmuSyncColors

/**
 * A dialog to edit the specific save paths for a single ROM (GameItem).
 */
@Composable
fun EditGameDialog(
    game: GameItem,
    onDismiss: () -> Unit,
    onSave: (List<String>) -> Unit,
) {
    var savePaths by remember {
        mutableStateOf(
            if (game.effectiveSavePaths.isEmpty()) listOf("")
            else game.effectiveSavePaths
        )
    }

    Dialog(onDismissRequest = onDismiss) {
        Surface(
            shape = RoundedCornerShape(16.dp),
            color = EmuSyncColors.Surface,
            modifier = Modifier
                .widthIn(min = 400.dp, max = 500.dp)
                .fillMaxHeight(0.8f)
        ) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(24.dp)
            ) {
                Text(
                    text = "Edit Game Saves",
                    style = MaterialTheme.typography.headlineSmall,
                    fontWeight = FontWeight.Bold,
                    color = EmuSyncColors.OnSurface,
                )
                Spacer(Modifier.height(8.dp))
                Text(
                    text = "Configure specific save files or directories for ${game.name}.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = EmuSyncColors.OnSurfaceDim,
                )
                Spacer(Modifier.height(20.dp))

                Column(
                    modifier = Modifier
                        .weight(1f)
                        .verticalScroll(rememberScrollState())
                ) {
                    Text(
                        text = "Save Files/Directories Paths",
                        style = MaterialTheme.typography.labelMedium,
                        color = EmuSyncColors.OnSurfaceDim,
                    )
                    Spacer(Modifier.height(4.dp))
                    
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
                                    Icon(Icons.Default.Delete, contentDescription = "Remove Path", tint = EmuSyncColors.Error)
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
                }

                Spacer(Modifier.height(20.dp))

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
                            onSave(validSavePaths)
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
