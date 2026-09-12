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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Gamepad
import androidx.compose.material.icons.filled.SportsEsports
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import com.emusync.model.EmulatorSystem
import com.emusync.model.GameEntry
import com.emusync.model.NativePCGame
import com.emusync.steam.SteamShortcutManager
import com.emusync.ui.components.FilePicker
import com.emusync.ui.components.PickerType
import com.emusync.ui.theme.EmuSyncColors

/**
 * Entry type selector for the add dialog.
 */
private enum class EntryType(val label: String) {
    EMULATOR("Emulator"),
    NATIVE("Native Game"),
}

/**
 * Dialog to add or edit an emulator system or native game entry.
 */
@Composable
fun AddEntryDialog(
    initialEntry: GameEntry? = null,
    onDismiss: () -> Unit,
    onSave: (GameEntry) -> Unit,
) {
    var entryType by remember {
        mutableStateOf(if (initialEntry is NativePCGame) EntryType.NATIVE else EntryType.EMULATOR)
    }
    var name by remember { mutableStateOf(initialEntry?.name ?: "") }

    // Shared & Type-specific fields
    var executablePath by remember {
        mutableStateOf(
            when (initialEntry) {
                is EmulatorSystem -> initialEntry.executablePath
                is NativePCGame -> initialEntry.executablePath
                else -> ""
            }
        )
    }
    var arguments by remember {
        mutableStateOf(
            when (initialEntry) {
                is EmulatorSystem -> initialEntry.arguments.joinToString(" ")
                is NativePCGame -> initialEntry.arguments.joinToString(" ")
                else -> if (entryType == EntryType.EMULATOR) "{ROM}" else ""
            }
        )
    }
    var romsDirectory by remember {
        mutableStateOf((initialEntry as? EmulatorSystem)?.romsDirectory ?: "")
    }
    var extensions by remember {
        mutableStateOf((initialEntry as? EmulatorSystem)?.extensions?.joinToString(", ") ?: "")
    }
    var savePaths by remember {
        mutableStateOf(
            if (initialEntry != null && initialEntry.savePaths.isNotEmpty()) initialEntry.savePaths
            else listOf("")
        )
    }
    var waitForProcess by remember {
        mutableStateOf((initialEntry as? NativePCGame)?.waitForProcess ?: "")
    }
    var fullscreenArgs by remember {
        mutableStateOf((initialEntry as? EmulatorSystem)?.fullscreenArgs ?: "")
    }
    var coversDirectory by remember {
        mutableStateOf((initialEntry as? EmulatorSystem)?.coversDirectory ?: "")
    }
    var coverPath by remember {
        mutableStateOf((initialEntry as? NativePCGame)?.coverPath ?: "")
    }

    // When switching types, set reasonable defaults for arguments
    LaunchedEffect(entryType) {
        if (entryType == EntryType.EMULATOR && arguments.isBlank()) arguments = "{ROM}"
        if (entryType == EntryType.NATIVE && arguments == "{ROM}") arguments = ""
    }

    var errorMessage by remember { mutableStateOf<String?>(null) }

    Dialog(onDismissRequest = onDismiss) {
        Surface(
            shape = RoundedCornerShape(20.dp),
            color = EmuSyncColors.Surface,
            border = BorderStroke(1.dp, EmuSyncColors.Divider),
            tonalElevation = 8.dp,
        ) {
            Column(
                modifier = Modifier
                    .padding(24.dp)
                    .widthIn(min = 500.dp, max = 600.dp)
                    .verticalScroll(rememberScrollState()),
            ) {
                // ── Header ──────────────────────────────────────────
                Text(
                    text = if (initialEntry == null) "Add New Entry" else "Edit Entry",
                    style = MaterialTheme.typography.titleLarge,
                    fontWeight = FontWeight.Bold,
                    color = EmuSyncColors.OnBackground,
                )
                Spacer(Modifier.height(20.dp))

                // ── Type selector ───────────────────────────────────
                Text(
                    text = "Type",
                    style = MaterialTheme.typography.labelMedium,
                    color = EmuSyncColors.OnSurfaceDim,
                )
                Spacer(Modifier.height(6.dp))
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    EntryType.entries.forEach { type ->
                        val selected = type == entryType
                        FilterChip(
                            selected = selected,
                            onClick = { entryType = type },
                            label = { Text(type.label) },
                            leadingIcon = {
                                Icon(
                                    imageVector = when (type) {
                                        EntryType.EMULATOR -> Icons.Default.SportsEsports
                                        EntryType.NATIVE -> Icons.Default.Gamepad
                                    },
                                    contentDescription = null,
                                    modifier = Modifier.size(18.dp),
                                )
                            },
                            colors = FilterChipDefaults.filterChipColors(
                                selectedContainerColor = EmuSyncColors.PrimaryContainer,
                                selectedLabelColor = EmuSyncColors.Primary,
                                selectedLeadingIconColor = EmuSyncColors.Primary,
                            ),
                            modifier = Modifier.weight(1f),
                        )
                    }
                }

                Spacer(Modifier.height(16.dp))

                // ── Name ────────────────────────────────────────────
                FormField(
                    label = "Name",
                    value = name,
                    onValueChange = { name = it },
                    placeholder = if (entryType == EntryType.EMULATOR) "e.g. Game Boy Advance" else "e.g. Spelunky Classic",
                )

                Spacer(Modifier.height(12.dp))

                // ── Executable Path ─────────────────────────────────
                PathField(
                    label = if (entryType == EntryType.EMULATOR) "Emulator Executable Path" else "Executable Path",
                    value = executablePath,
                    onValueChange = { executablePath = it },
                    placeholder = if (entryType == EntryType.EMULATOR) "/usr/bin/mgba-qt" else "/opt/spelunky/spelunky",
                    pickerType = PickerType.FILE,
                )
                Spacer(Modifier.height(12.dp))

                // ── Arguments ───────────────────────────────────────
                FormField(
                    label = "Arguments (space-separated)",
                    value = arguments,
                    onValueChange = { arguments = it },
                    placeholder = if (entryType == EntryType.EMULATOR) "{ROM} --fullscreen" else "--fullscreen --no-intro",
                )
                if (entryType == EntryType.EMULATOR) {
                    Text(
                        text = "Use {ROM} to insert the requested game's path.",
                        style = MaterialTheme.typography.bodySmall,
                        color = EmuSyncColors.OnSurfaceDim,
                        modifier = Modifier.padding(top = 4.dp, start = 4.dp)
                    )
                }
                Spacer(Modifier.height(12.dp))

                // ── Emulator specifics ──────────────────────────────
                if (entryType == EntryType.EMULATOR) {
                    PathField(
                        label = "ROMs Directory",
                        value = romsDirectory,
                        onValueChange = { romsDirectory = it },
                        placeholder = "/home/user/roms/gba",
                        pickerType = PickerType.DIRECTORY,
                    )
                    Spacer(Modifier.height(12.dp))

                    FormField(
                        label = "File Extensions (comma-separated)",
                        value = extensions,
                        onValueChange = { extensions = it },
                        placeholder = "gba, gbc",
                    )
                    Spacer(Modifier.height(12.dp))

                    FormField(
                        label = "Fullscreen / Big Picture Flags (auto-detected if empty)",
                        value = fullscreenArgs,
                        onValueChange = { fullscreenArgs = it },
                        placeholder = SteamShortcutManager.detectFullscreenArgs(executablePath)
                            .ifEmpty { "e.g. -bigpicture -fullscreen" },
                    )
                    Text(
                        text = "Forces the emulator to bypass its Qt GUI. Auto-detected: \"${
                            SteamShortcutManager.detectFullscreenArgs(executablePath).ifEmpty { "(none)" }
                        }\"",
                        style = MaterialTheme.typography.bodySmall,
                        color = EmuSyncColors.OnSurfaceDim,
                        modifier = Modifier.padding(top = 4.dp, start = 4.dp)
                    )
                    Spacer(Modifier.height(12.dp))

                    PathField(
                        label = "Covers Directory (Optional)",
                        value = coversDirectory,
                        onValueChange = { coversDirectory = it },
                        placeholder = "/path/to/covers (empty to use ROMs folder)",
                        pickerType = PickerType.DIRECTORY,
                    )
                    Spacer(Modifier.height(12.dp))
                }

                if (entryType == EntryType.NATIVE) {
                    FormField(
                        label = "Wait for Process Name (Optional)",
                        value = waitForProcess,
                        onValueChange = { waitForProcess = it },
                        placeholder = "Spelunky.exe",
                    )
                    Text(
                        text = "Useful for Steam games: EmuSync will wait for this process to close.",
                        style = MaterialTheme.typography.bodySmall,
                        color = EmuSyncColors.OnSurfaceDim,
                        modifier = Modifier.padding(horizontal = 4.dp)
                    )
                    Spacer(Modifier.height(12.dp))

                    PathField(
                        label = "Cover Image (Optional)",
                        value = coverPath,
                        onValueChange = { coverPath = it },
                        placeholder = "/path/to/game_cover.png",
                        pickerType = PickerType.FILE,
                    )
                    Spacer(Modifier.height(12.dp))
                }

                // ── Save Paths (Applies to both Native & Emulator globals)
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
                            PathField(
                                label = null,
                                value = path,
                                onValueChange = { newVal ->
                                    val newList = savePaths.toMutableList()
                                    newList[index] = newVal
                                    savePaths = newList
                                },
                                placeholder = if (entryType == EntryType.EMULATOR)
                                    "/home/user/emulator/memcards/Mcd001.ps2"
                                else
                                    "/home/user/.local/share/save.dat",
                                pickerType = PickerType.ANY,
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

                // ── Error message ───────────────────────────────────
                if (errorMessage != null) {
                    Spacer(Modifier.height(8.dp))
                    Text(
                        text = errorMessage!!,
                        style = MaterialTheme.typography.bodySmall,
                        color = EmuSyncColors.Error,
                    )
                }

                Spacer(Modifier.height(20.dp))

                // ── Buttons ─────────────────────────────────────────
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
                            val entry = validateAndBuild(
                                type = entryType,
                                name = name.trim(),
                                executablePath = executablePath.trim(),
                                arguments = arguments.trim(),
                                romsDirectory = romsDirectory.trim(),
                                extensions = extensions.trim(),
                                savePaths = validSavePaths,
                                waitForProcess = waitForProcess.trim(),
                                fullscreenArgs = fullscreenArgs.trim(),
                                coversDirectory = coversDirectory.trim(),
                                coverPath = coverPath.trim(),
                            )
                            if (entry != null) {
                                // Preserve driveFileId and steamAppId if editing
                                val updatedEntry = when (entry) {
                                    is EmulatorSystem -> entry.copy(
                                        driveFileId = initialEntry?.driveFileId,
                                        steamAppId = initialEntry?.steamAppId,
                                        steamProcessName = (initialEntry as? EmulatorSystem)?.steamProcessName,
                                    )
                                    is NativePCGame -> entry.copy(
                                        driveFileId = initialEntry?.driveFileId,
                                        steamAppId = initialEntry?.steamAppId,
                                    )
                                }
                                onSave(updatedEntry)
                            } else {
                                errorMessage = "Please fill in all required fields."
                            }
                        },
                        colors = ButtonDefaults.buttonColors(
                            containerColor = EmuSyncColors.Primary,
                        ),
                    ) {
                        Text(if (initialEntry == null) "Add" else "Save")
                    }
                }
            }
        }
    }
}

@Composable
private fun FormField(
    label: String,
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
) {
    Column {
        Text(
            text = label,
            style = MaterialTheme.typography.labelMedium,
            color = EmuSyncColors.OnSurfaceDim,
        )
        Spacer(Modifier.height(4.dp))
        OutlinedTextField(
            value = value,
            onValueChange = onValueChange,
            modifier = Modifier.fillMaxWidth(),
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
    }
}

@Composable
private fun PathField(
    label: String?,
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
    pickerType: PickerType
) {
    Column {
        if (label != null) {
            Text(
                text = label,
                style = MaterialTheme.typography.labelMedium,
                color = EmuSyncColors.OnSurfaceDim,
            )
            Spacer(Modifier.height(4.dp))
        }
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
            if (pickerType == PickerType.FILE || pickerType == PickerType.ANY) {
                Button(
                    onClick = {
                        val result = FilePicker.pickPath(
                            label = label ?: "File",
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
                    Text(if (pickerType == PickerType.ANY) "File" else "Browse")
                }
            }
            if (pickerType == PickerType.DIRECTORY || pickerType == PickerType.ANY) {
                if (pickerType == PickerType.ANY) Spacer(Modifier.width(8.dp))
                Button(
                    onClick = {
                        val result = FilePicker.pickPath(
                            label = label ?: "Folder",
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
                    Text(if (pickerType == PickerType.ANY) "Folder" else "Browse")
                }
            }
        }
    }
}

private fun validateAndBuild(
    type: EntryType,
    name: String,
    executablePath: String,
    arguments: String,
    romsDirectory: String,
    extensions: String,
    savePaths: List<String>,
    waitForProcess: String,
    fullscreenArgs: String,
    coversDirectory: String = "",
    coverPath: String = "",
): GameEntry? {
    if (name.isBlank() || executablePath.isBlank()) return null
    
    val validSavePaths = savePaths.map { it.trim() }.filter { it.isNotBlank() }
    if (type == EntryType.NATIVE && validSavePaths.isEmpty()) return null

    val argList = if (arguments.isBlank()) emptyList()
    else arguments.split(" ").map { it.trim() }.filter { it.isNotBlank() }

    return when (type) {
        EntryType.EMULATOR -> {
            if (romsDirectory.isBlank() || extensions.isBlank()) return null
            val extList = extensions.split(",", " ").map { it.trim() }.filter { it.isNotBlank() }
            if (extList.isEmpty()) return null
            EmulatorSystem(
                name = name,
                executablePath = executablePath,
                arguments = argList,
                romsDirectory = romsDirectory,
                extensions = extList,
                savePaths = validSavePaths,
                fullscreenArgs = fullscreenArgs.trim().takeIf { it.isNotBlank() },
                coversDirectory = coversDirectory.trim().takeIf { it.isNotBlank() },
            )
        }
        EntryType.NATIVE -> {
            NativePCGame(
                name = name,
                executablePath = executablePath,
                arguments = argList,
                savePaths = validSavePaths,
                waitForProcess = waitForProcess.takeIf { it.isNotBlank() },
                coverPath = coverPath.trim().takeIf { it.isNotBlank() },
            )
        }
    }
}
