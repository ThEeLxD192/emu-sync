package com.emusync.ui.dialogs

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.emusync.model.EmulatorSystem
import com.emusync.model.SaveTargetType
import com.emusync.ui.GameItem
import com.emusync.ui.components.CloudPathField
import com.emusync.ui.components.PickerType
import com.emusync.ui.theme.EmuSyncColors

@Composable
internal fun ColumnScope.EntrySaveFields(form: EntryFormState) {
    val entryType = form.entryType
    with(form.saves) {
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
                        onValueChange = { newVal -> updatePath(index, newVal) },
                        onTypeChange = { newType -> updateType(index, newType) },
                        placeholder =
                            if (entryType == EntryType.EMULATOR)
                                "/home/user/emulator/memcards/Mcd001.ps2"
                            else "/home/user/.local/share/save.dat",
                        pickerType = PickerType.ANY,
                    )
                }
                TextButton(onClick = { toggleType(index) }) {
                    Text(
                        when (saveTypes[index]) {
                            SaveTargetType.FILE -> "File"
                            SaveTargetType.DIRECTORY -> "Folder"
                            SaveTargetType.UNKNOWN -> "Choose type"
                        }
                    )
                }
                if (savePaths.size > 1) {
                    Spacer(Modifier.width(8.dp))
                    IconButton(onClick = { removePath(index) }) {
                        Icon(
                            Icons.Default.Delete,
                            contentDescription = "Remove Path",
                            tint = EmuSyncColors.Error,
                        )
                    }
                }
            }
            CloudPathField(
                value = saveCloudPaths.getOrElse(index) { "" },
                onValueChange = { value -> updateCloudPath(index, value) },
                type = saveTypes[index],
            )
            Spacer(Modifier.height(8.dp))
        }

        TextButton(
            onClick = { addPath() },
            modifier = Modifier.align(Alignment.Start),
        ) {
            Text("+ Add Another Path", color = EmuSyncColors.Primary)
        }
    }
}

@Composable
internal fun ColumnScope.GameSaveFields(
    form: GameEditFormState,
    game: GameItem,
    cloudRestoreFileNames: List<String>,
    selectedRestorePaths: List<String>,
) {
    with(form) {
        // ── Save Paths Section ────────────────────────────
        Text(
            text =
                if (cloudRestoreFileNames.isNotEmpty()) "Cloud Save Destination"
                else "Save Files/Directories Paths",
            style = MaterialTheme.typography.titleSmall,
            fontWeight = FontWeight.SemiBold,
            color = EmuSyncColors.OnSurface,
        )
        Spacer(Modifier.height(4.dp))
        Text(
            text =
                if (cloudRestoreFileNames.isNotEmpty()) {
                    "Choose one folder. The cloud files below will be restored as individual files; other files in this folder will not be synced."
                } else if (game.entry is EmulatorSystem) {
                    "Only paths specific to this game. Emulator-wide paths are configured in the system settings."
                } else {
                    "Specific save files or directories synchronized for this game."
                },
            style = MaterialTheme.typography.bodySmall,
            color = EmuSyncColors.OnSurfaceDim,
        )
        Spacer(Modifier.height(10.dp))

        if (cloudRestoreFileNames.isNotEmpty())
            GameRestoreFields(cloudRestoreFileNames, selectedRestorePaths)
        else saves.GameSaveRows()
        if (cloudRestoreFileNames.isEmpty()) {
            TextButton(
                onClick = {
                    saves.addPath()
                },
                modifier = Modifier.align(Alignment.Start),
            ) {
                Text("+ Add Another Path", color = EmuSyncColors.Primary)
            }
        }

        Spacer(Modifier.height(20.dp))
    }
}

@Composable
private fun GameEditFormState.GameRestoreFields(
    cloudRestoreFileNames: List<String>,
    selectedRestorePaths: List<String>,
) {

    Row(
        modifier = Modifier.fillMaxWidth().clickable { restoreWholeFolder = false },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        RadioButton(
            selected = !restoreWholeFolder,
            onClick = { restoreWholeFolder = false },
        )
        Text("Only the cloud files listed below", color = EmuSyncColors.OnSurface)
    }
    Row(
        modifier = Modifier.fillMaxWidth().clickable { restoreWholeFolder = true },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        RadioButton(
            selected = restoreWholeFolder,
            onClick = { restoreWholeFolder = true },
        )
        Text("The entire folder", color = EmuSyncColors.OnSurface)
    }
    Text(
        text =
            if (restoreWholeFolder) {
                "Every file in this folder will be synchronized. Use a dedicated folder if it contains saves from other games."
            } else {
                "Only the cloud files listed below will be synchronized; other files in the folder will be left alone."
            },
        style = MaterialTheme.typography.bodySmall,
        color = EmuSyncColors.OnSurfaceDim,
        modifier = Modifier.padding(start = 8.dp, bottom = 8.dp),
    )
    EditPathField(
        value = restoreDestinationFolder,
        onValueChange = { restoreDestinationFolder = it },
        placeholder = "Choose a destination folder with the Folder button",
        allowFileSelection = false,
    )
    if (selectedRestorePaths.isEmpty()) {
        Text(
            text = "Select a destination folder before saving.",
            style = MaterialTheme.typography.bodySmall,
            color = EmuSyncColors.Error,
            modifier = Modifier.padding(top = 6.dp),
        )
    }
    Spacer(Modifier.height(10.dp))
    cloudRestoreFileNames.forEach { fileName ->
        Text(
            text = "• $fileName",
            style = MaterialTheme.typography.bodySmall,
            color = EmuSyncColors.OnSurfaceDim,
            modifier = Modifier.padding(start = 8.dp, bottom = 3.dp),
        )
    }
}

@Composable
private fun SaveTargetFormState.GameSaveRows() {
    savePaths.forEachIndexed { index, path ->
        Row(
            modifier = Modifier.fillMaxWidth().padding(bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(modifier = Modifier.weight(1f)) {
                EditPathField(
                    value = path,
                    onValueChange = { newVal -> updatePath(index, newVal) },
                    onTypeChange = { newType -> updateType(index, newType) },
                    placeholder = "/home/user/saves/game.sav",
                )
            }
            TextButton(onClick = { toggleType(index) }) {
                Text(
                    when (saveTypes[index]) {
                        SaveTargetType.FILE -> "File"
                        SaveTargetType.DIRECTORY -> "Folder"
                        SaveTargetType.UNKNOWN -> "Choose type"
                    }
                )
            }
            if (savePaths.size > 1) {
                Spacer(Modifier.width(8.dp))
                IconButton(onClick = { removePath(index) }) {
                    Icon(
                        Icons.Default.Delete,
                        contentDescription = "Remove Path",
                        tint = EmuSyncColors.Error,
                    )
                }
            }
        }
        CloudPathField(
            value = saveCloudPaths.getOrElse(index) { "" },
            onValueChange = { value -> updateCloudPath(index, value) },
            type = saveTypes[index],
        )
        Spacer(Modifier.height(8.dp))
    }
}
