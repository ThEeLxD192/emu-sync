package com.emusync.ui.library.form

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Gamepad
import androidx.compose.material.icons.filled.SportsEsports
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import com.emusync.domain.execution.PickerType
import com.emusync.domain.library.GameEntry
import com.emusync.ui.theme.EmuSyncColors

/** Dialog to add or edit an emulator system or native game entry. */
@Composable
fun AddEntryDialog(
    actions: LibraryFormActions,
    initialEntry: GameEntry? = null,
    existingGroups: List<String> = emptyList(),
    onDismiss: () -> Unit,
    onSave: (GameEntry) -> Unit,
) {
    val form = remember { EntryFormState(initialEntry, actions.saveTargets) }
    LaunchedEffect(form.entryType) { form.applyTypeDefaults() }

    Dialog(onDismissRequest = onDismiss) {
        Surface(
            shape = RoundedCornerShape(20.dp),
            color = EmuSyncColors.Surface,
            border = BorderStroke(1.dp, EmuSyncColors.Divider),
            tonalElevation = 8.dp,
        ) {
            Column(
                modifier =
                    Modifier.padding(24.dp)
                        .widthIn(min = 500.dp, max = 600.dp)
                        .verticalScroll(rememberScrollState())
            ) {
                // ── Header ──────────────────────────────────────────
                Text(
                    text = if (initialEntry == null) "Add New Entry" else "Edit Entry",
                    style = MaterialTheme.typography.titleLarge,
                    fontWeight = FontWeight.Bold,
                    color = EmuSyncColors.OnBackground,
                )
                Spacer(Modifier.height(20.dp))

                form.EntryTypeSelector()
                form.EntryIdentityFields(existingGroups)
                form.EntryLaunchFields(actions = actions)
                form.EmulatorFields(actions = actions)
                form.NativeFields(actions = actions)
                EntrySaveFields(actions = actions, form)
                form.errorMessage?.let { message ->
                    Spacer(Modifier.height(8.dp))
                    Text(
                        text = message,
                        style = MaterialTheme.typography.bodySmall,
                        color = EmuSyncColors.Error,
                    )
                }
                Spacer(Modifier.height(20.dp))
                form.EntryActions(initialEntry, onDismiss, onSave)
            }
        }
    }
}

@Composable
private fun EntryFormState.EntryTypeSelector() {
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
                        imageVector =
                            when (type) {
                                EntryType.EMULATOR -> Icons.Default.SportsEsports
                                EntryType.NATIVE -> Icons.Default.Gamepad
                            },
                        contentDescription = null,
                        modifier = Modifier.size(18.dp),
                    )
                },
                colors =
                    FilterChipDefaults.filterChipColors(
                        selectedContainerColor = EmuSyncColors.PrimaryContainer,
                        selectedLabelColor = EmuSyncColors.Primary,
                        selectedLeadingIconColor = EmuSyncColors.Primary,
                    ),
                modifier = Modifier.weight(1f),
            )
        }
    }

    Spacer(Modifier.height(16.dp))
}

@Composable
private fun EntryFormState.EntryIdentityFields(existingGroups: List<String>) {
    // ── Name ────────────────────────────────────────────
    FormField(
        label = "Name",
        value = name,
        onValueChange = { name = it },
        placeholder =
            if (entryType == EntryType.EMULATOR) "e.g. Yuzu or Eden" else "e.g. Spelunky Classic",
    )

    Spacer(Modifier.height(12.dp))

    // ── Group / Platform Folder ──────────────────────────
    FormField(
        label = "Folder / Platform Group (Optional)",
        value = group,
        onValueChange = { group = it },
        placeholder = "e.g. Switch, PlayStation (shares cloud saves folder)",
    )
    if (existingGroups.isNotEmpty()) {
        Spacer(Modifier.height(4.dp))
        Text(
            text = "Assign to existing folder:",
            style = MaterialTheme.typography.labelSmall,
            color = EmuSyncColors.OnSurfaceDim,
        )
        Spacer(Modifier.height(4.dp))
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            existingGroups.forEach { grp ->
                FilterChip(
                    selected = group.equals(grp, ignoreCase = true),
                    onClick = {
                        group = if (group.equals(grp, ignoreCase = true)) "" else grp
                    },
                    label = { Text(grp) },
                    colors =
                        FilterChipDefaults.filterChipColors(
                            selectedContainerColor = EmuSyncColors.PrimaryContainer,
                            selectedLabelColor = EmuSyncColors.Primary,
                            containerColor = EmuSyncColors.SurfaceVariant,
                            labelColor = EmuSyncColors.OnSurface,
                        ),
                )
            }
        }
    }
    val previewCloud = cloudFolder.ifBlank { group.ifBlank { name.ifBlank { "..." } } }
    Text(
        text = "☁️ Google Drive saves folder: \"$previewCloud\"",
        style = MaterialTheme.typography.bodySmall,
        color = if (group.isNotBlank()) EmuSyncColors.Primary else EmuSyncColors.OnSurfaceDim,
        modifier = Modifier.padding(top = 4.dp, start = 4.dp),
    )

    Spacer(Modifier.height(12.dp))
}

@Composable
private fun EntryFormState.EntryLaunchFields(actions: LibraryFormActions) {
    // ── Executable Path ─────────────────────────────────
    PathField(
        actions = actions,
        label =
            if (entryType == EntryType.EMULATOR) "Emulator Executable Path" else "Executable Path",
        value = executablePath,
        onValueChange = { executablePath = it },
        placeholder =
            if (entryType == EntryType.EMULATOR) "/usr/bin/mgba-qt" else "/opt/spelunky/spelunky",
        pickerType = PickerType.FILE,
    )
    Spacer(Modifier.height(12.dp))

    // ── Arguments ───────────────────────────────────────
    FormField(
        label = "Arguments (quote values that contain spaces)",
        value = arguments,
        onValueChange = { arguments = it },
        placeholder =
            if (entryType == EntryType.EMULATOR) "{ROM} --fullscreen"
            else "--fullscreen --no-intro",
    )
    Text(
        text = "Example: --profile \"My Profile\" keeps My Profile as one argument.",
        style = MaterialTheme.typography.bodySmall,
        color = EmuSyncColors.OnSurfaceDim,
        modifier = Modifier.padding(top = 4.dp, start = 4.dp),
    )
    if (entryType == EntryType.EMULATOR) {
        Text(
            text = "Use {ROM} to insert the requested game's path.",
            style = MaterialTheme.typography.bodySmall,
            color = EmuSyncColors.OnSurfaceDim,
            modifier = Modifier.padding(top = 4.dp, start = 4.dp),
        )
    }
    Spacer(Modifier.height(12.dp))
}

@Composable
private fun EntryFormState.EmulatorFields(actions: LibraryFormActions) {
    // ── Emulator specifics ──────────────────────────────
    if (entryType == EntryType.EMULATOR) {
        PathField(
            actions = actions,
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
            placeholder =
                actions.detectFullscreenArgs(executablePath).ifEmpty {
                    "e.g. -bigpicture -fullscreen"
                },
        )
        Text(
            text =
                "Forces the emulator to bypass its Qt GUI. Auto-detected: \"${
                            actions.detectFullscreenArgs(executablePath).ifEmpty { "(none)" }
                        }\"",
            style = MaterialTheme.typography.bodySmall,
            color = EmuSyncColors.OnSurfaceDim,
            modifier = Modifier.padding(top = 4.dp, start = 4.dp),
        )
        Spacer(Modifier.height(12.dp))
    }
}

@Composable
private fun EntryFormState.NativeFields(actions: LibraryFormActions) {
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
            modifier = Modifier.padding(horizontal = 4.dp),
        )
        Spacer(Modifier.height(12.dp))

        PathField(
            actions = actions,
            label = "Cover Image (Optional)",
            value = coverPath,
            onValueChange = { coverPath = it },
            placeholder = "/path/to/game_cover.png",
            pickerType = PickerType.FILE,
        )
        Spacer(Modifier.height(12.dp))
    }
}

@Composable
private fun EntryFormState.EntryActions(
    initialEntry: GameEntry?,
    onDismiss: () -> Unit,
    onSave: (GameEntry) -> Unit,
) {
    // ── Buttons ─────────────────────────────────────────
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
    ) {
        TextButton(onClick = onDismiss) {
            Text("Cancel")
        }
        Button(
            onClick = { buildEntry(initialEntry)?.let(onSave) },
            colors = ButtonDefaults.buttonColors(containerColor = EmuSyncColors.Primary),
        ) {
            Text(if (initialEntry == null) "Add" else "Save")
        }
    }
}
