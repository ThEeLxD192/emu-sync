package com.emusync.ui.dialogs

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.emusync.model.SaveTargetType
import com.emusync.ui.components.PickerType
import com.emusync.ui.theme.EmuSyncColors
import kotlinx.coroutines.launch

@Composable
internal fun FormField(
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
            colors =
                OutlinedTextFieldDefaults.colors(
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
internal fun PathField(
    label: String?,
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
    pickerType: PickerType,
    onTypeChange: ((SaveTargetType) -> Unit)? = null,
) {
    val coroutineScope = rememberCoroutineScope()
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
                colors =
                    OutlinedTextFieldDefaults.colors(
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
                        coroutineScope.launch {
                            val result =
                                pickDialogPath(
                                    label = label ?: "File",
                                    initialPath = value,
                                    type = PickerType.FILE,
                                )
                            if (result != null) {
                                onValueChange(result)
                                onTypeChange?.invoke(SaveTargetType.FILE)
                            }
                        }
                    },
                    colors =
                        ButtonDefaults.buttonColors(
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
                        coroutineScope.launch {
                            val result =
                                pickDialogPath(
                                    label = label ?: "Folder",
                                    initialPath = value,
                                    type = PickerType.DIRECTORY,
                                )
                            if (result != null) {
                                onValueChange(result)
                                onTypeChange?.invoke(SaveTargetType.DIRECTORY)
                            }
                        }
                    },
                    colors =
                        ButtonDefaults.buttonColors(
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

@Composable
internal fun EditPathField(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
    allowFileSelection: Boolean = true,
    onTypeChange: ((SaveTargetType) -> Unit)? = null,
) {
    val coroutineScope = rememberCoroutineScope()
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
            colors =
                OutlinedTextFieldDefaults.colors(
                    focusedBorderColor = EmuSyncColors.Primary,
                    unfocusedBorderColor = EmuSyncColors.Divider,
                    cursorColor = EmuSyncColors.Primary,
                    focusedTextColor = EmuSyncColors.OnBackground,
                    unfocusedTextColor = EmuSyncColors.OnSurface,
                ),
            shape = RoundedCornerShape(10.dp),
        )
        Spacer(Modifier.width(8.dp))
        if (allowFileSelection)
            Button(
                onClick = {
                    coroutineScope.launch {
                        val result =
                            pickDialogPath(
                                label = "File",
                                initialPath = value,
                                type = PickerType.FILE,
                            )
                        if (result != null) {
                            onValueChange(result)
                            onTypeChange?.invoke(SaveTargetType.FILE)
                        }
                    }
                },
                colors =
                    ButtonDefaults.buttonColors(
                        containerColor = EmuSyncColors.SurfaceSelected,
                        contentColor = EmuSyncColors.OnSurface,
                    ),
                shape = RoundedCornerShape(10.dp),
                modifier = Modifier.height(56.dp),
            ) {
                Text("File")
            }
        if (allowFileSelection) Spacer(Modifier.width(8.dp))
        Button(
            onClick = {
                coroutineScope.launch {
                    val result =
                        pickDialogPath(
                            label = "Folder",
                            initialPath = value,
                            type = PickerType.DIRECTORY,
                        )
                    if (result != null) {
                        onValueChange(result)
                        onTypeChange?.invoke(SaveTargetType.DIRECTORY)
                    }
                }
            },
            colors =
                ButtonDefaults.buttonColors(
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
