package com.emusync.ui.components

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.emusync.drive.remotePathSegments
import com.emusync.model.SaveTargetType

@Composable
internal fun CloudPathField(value: String, onValueChange: (String) -> Unit, type: SaveTargetType) {
    val invalid = value.isNotBlank() && remotePathSegments(value.trim()) == null
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        modifier = Modifier.fillMaxWidth(),
        singleLine = true,
        label = { Text("Cloud path (optional)") },
        placeholder = {
            Text(if (type == SaveTargetType.DIRECTORY) "Folder prefix shared across devices" else "File name, including any subfolders")
        },
        isError = invalid,
        supportingText = {
            Text(if (invalid) "Use a relative path without '..'." else "Use the same cloud path for this save on each device.")
        },
    )
}
