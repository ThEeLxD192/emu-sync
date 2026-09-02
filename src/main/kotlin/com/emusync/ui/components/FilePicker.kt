package com.emusync.ui.components

import java.awt.FileDialog
import java.awt.Frame
import java.io.File

enum class PickerType {
    FILE, DIRECTORY, ANY
}

object FilePicker {

    /**
     * Opens a file or directory picker.
     * Uses AWT FileDialog for files (native portal), and kdialog/zenity for directories.
     */
    fun pickPath(
        label: String,
        initialPath: String,
        type: PickerType
    ): String? {
        val startFile = File(initialPath.ifBlank { System.getProperty("user.home") })
        val parentDir = if (startFile.isDirectory) startFile else startFile.parentFile ?: File(System.getProperty("user.home"))

        return when (type) {
            PickerType.FILE -> {
                // AWT FileDialog is native and best for files on Linux
                val dialog = FileDialog(null as Frame?, "Select $label", FileDialog.LOAD).apply {
                    directory = parentDir.absolutePath
                    if (startFile.isFile) file = startFile.name
                    isVisible = true
                }
                if (dialog.file != null) {
                    File(dialog.directory, dialog.file).absolutePath
                } else null
            }
            PickerType.DIRECTORY -> {
                // On SteamOS/KDE, kdialog is the native way to select directories.
                try {
                    val kdialog = ProcessBuilder("kdialog", "--title", "Select $label", "--getexistingdirectory", parentDir.absolutePath)
                        .start()
                    if (kdialog.waitFor() == 0) {
                        val path = kdialog.inputStream.bufferedReader().readText().trim()
                        if (path.isNotEmpty()) return path
                    }
                } catch (_: Exception) { /* kdialog not found or failed */ }

                // On GNOME/Arch, zenity is the native way.
                try {
                    val zenity = ProcessBuilder("zenity", "--file-selection", "--directory", "--title", "Select $label", "--filename=${parentDir.absolutePath}/")
                        .start()
                    if (zenity.waitFor() == 0) {
                        val path = zenity.inputStream.bufferedReader().readText().trim()
                        if (path.isNotEmpty()) return path
                    }
                } catch (_: Exception) { /* zenity not found or failed */ }
                
                null
            }
            PickerType.ANY -> null // ANY is only used by the UI to show both buttons
        }
    }
}
