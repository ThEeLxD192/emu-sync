package com.emusync.ui.components

import java.awt.EventQueue
import java.awt.FileDialog
import java.awt.Frame
import java.io.File
import javax.swing.JFileChooser
import javax.swing.UIManager

enum class PickerType {
    FILE, DIRECTORY, ANY
}

object FilePicker {

    /**
     * Provider for checking if the running OS is Windows.
     * Can be replaced in tests.
     */
    var isWindowsProvider: () -> Boolean = {
        System.getProperty("os.name", "").lowercase().contains("windows")
    }

    /**
     * Delegate for file selection. Can be replaced in tests.
     */
    var fileChooser: (label: String, parentDir: File, startFile: File) -> String? = ::defaultFileChooser

    /**
     * Delegate for Linux external directory selection (kdialog/zenity). Can be replaced in tests.
     */
    var linuxExternalDirectoryChooser: (label: String, parentDir: File) -> String? = ::defaultLinuxDirectoryChooser

    /**
     * Delegate for Swing directory selection. Can be replaced in tests.
     */
    var swingDirectoryChooser: (label: String, parentDir: File) -> String? = ::defaultSwingFolderChooser

    /**
     * Opens a file or directory picker.
     * Uses AWT FileDialog for files (native dialog), and:
     * - On Windows: Swing JFileChooser with system look and feel.
     * - On Linux: kdialog -> zenity -> fallback to JFileChooser.
     */
    fun pickPath(
        label: String,
        initialPath: String,
        type: PickerType
    ): String? {
        val startFile = File(initialPath.ifBlank { System.getProperty("user.home") })
        val parentDir = if (startFile.isDirectory) startFile else startFile.parentFile ?: File(System.getProperty("user.home"))

        return when (type) {
            PickerType.FILE -> fileChooser(label, parentDir, startFile)
            PickerType.DIRECTORY -> pickDirectory(label, parentDir)
            PickerType.ANY -> null // ANY is only used by the UI to show both buttons
        }
    }

    private fun defaultFileChooser(label: String, parentDir: File, startFile: File): String? {
        val dialog = FileDialog(null as Frame?, "Select $label", FileDialog.LOAD).apply {
            directory = parentDir.absolutePath
            if (startFile.isFile) file = startFile.name
            isVisible = true
        }
        return if (dialog.file != null) {
            File(dialog.directory, dialog.file).absolutePath
        } else null
    }

    fun pickDirectory(label: String, parentDir: File): String? {
        if (isWindowsProvider()) {
            return pickDirectorySwing(label, parentDir)
        }

        val externalResult = linuxExternalDirectoryChooser(label, parentDir)
        if (externalResult != null) return externalResult

        // Universal fallback: Swing JFileChooser
        return pickDirectorySwing(label, parentDir)
    }

    private fun defaultLinuxDirectoryChooser(label: String, parentDir: File): String? {
        // On Linux / SteamOS KDE: try kdialog first
        try {
            val kdialog = ProcessBuilder("kdialog", "--title", "Select $label", "--getexistingdirectory", parentDir.absolutePath)
                .start()
            if (kdialog.waitFor() == 0) {
                val path = kdialog.inputStream.bufferedReader().readText().trim()
                if (path.isNotEmpty()) return path
            }
        } catch (_: Exception) { /* kdialog not found or failed */ }

        // On GNOME / other Linux: try zenity
        try {
            val zenity = ProcessBuilder("zenity", "--file-selection", "--directory", "--title", "Select $label", "--filename=${parentDir.absolutePath}/")
                .start()
            if (zenity.waitFor() == 0) {
                val path = zenity.inputStream.bufferedReader().readText().trim()
                if (path.isNotEmpty()) return path
            }
        } catch (_: Exception) { /* zenity not found or failed */ }

        return null
    }

    fun pickDirectorySwing(label: String, parentDir: File): String? {
        return try {
            if (!EventQueue.isDispatchThread()) {
                var selected: String? = null
                EventQueue.invokeAndWait {
                    selected = swingDirectoryChooser(label, parentDir)
                }
                selected
            } else {
                swingDirectoryChooser(label, parentDir)
            }
        } catch (_: Throwable) {
            null
        }
    }

    private fun defaultSwingFolderChooser(label: String, parentDir: File): String? {
        try {
            UIManager.setLookAndFeel(UIManager.getSystemLookAndFeelClassName())
        } catch (_: Throwable) { /* Ignore L&F issues */ }

        val chooser = JFileChooser(parentDir).apply {
            dialogTitle = "Select $label"
            fileSelectionMode = JFileChooser.DIRECTORIES_ONLY
            isAcceptAllFileFilterUsed = false
        }

        val result = chooser.showOpenDialog(null)
        return if (result == JFileChooser.APPROVE_OPTION && chooser.selectedFile != null) {
            chooser.selectedFile.absolutePath
        } else {
            null
        }
    }
}
