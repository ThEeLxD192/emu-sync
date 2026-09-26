package com.emusync.ui.components

import java.awt.Dialog
import java.awt.EventQueue
import java.awt.FileDialog
import java.awt.Frame
import java.awt.KeyboardFocusManager
import java.awt.Window
import java.io.File
import javax.swing.JFileChooser
import javax.swing.UIManager

enum class PickerType {
    FILE, DIRECTORY, ANY
}

sealed class ExternalChooserResult {
    data class Selected(val path: String) : ExternalChooserResult()
    object Cancelled : ExternalChooserResult()
    object NotAvailable : ExternalChooserResult()
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
     * Delegate for AWT/native file selection. Can be replaced in tests.
     */
    var fileChooser: (label: String, parentDir: File, startFile: File) -> String? = ::defaultFileChooser

    /**
     * Delegate for Swing directory selection. Can be replaced in tests.
     */
    var swingDirectoryChooser: (label: String, parentDir: File) -> String? = ::defaultSwingFolderChooser

    /**
     * Delegate for Linux external directory selection (kdialog/zenity). Can be replaced in tests.
     */
    var linuxExternalDirectoryChooser: (label: String, parentDir: File) -> ExternalChooserResult = ::defaultLinuxDirectoryChooser

    /**
     * Delegate for Linux external file selection (kdialog/zenity). Can be replaced in tests.
     */
    var linuxExternalFileChooser: (label: String, parentDir: File, startFile: File) -> ExternalChooserResult = ::defaultLinuxFileChooser

    /**
     * Opens a file or directory picker.
     * - On Linux: kdialog -> zenity -> fallback to AWT/Swing.
     *   If user cancels, returns null immediately without fallback.
     * - On Windows: AWT FileDialog for files, Swing JFileChooser for directories.
     */
    fun pickPath(
        label: String,
        initialPath: String,
        type: PickerType
    ): String? {
        val startFile = File(initialPath.ifBlank { System.getProperty("user.home") })
        val parentDir = if (startFile.isDirectory) startFile else startFile.parentFile ?: File(System.getProperty("user.home"))

        return when (type) {
            PickerType.FILE -> pickFile(label, parentDir, startFile)
            PickerType.DIRECTORY -> pickDirectory(label, parentDir)
            PickerType.ANY -> null // ANY is only used by the UI to show both buttons
        }
    }

    fun pickFile(label: String, parentDir: File, startFile: File): String? {
        if (isWindowsProvider()) {
            return pickFileAwt(label, parentDir, startFile)
        }

        when (val externalResult = linuxExternalFileChooser(label, parentDir, startFile)) {
            is ExternalChooserResult.Selected -> return externalResult.path
            is ExternalChooserResult.Cancelled -> return null
            is ExternalChooserResult.NotAvailable -> { /* fallback to AWT */ }
        }

        return pickFileAwt(label, parentDir, startFile)
    }

    fun pickDirectory(label: String, parentDir: File): String? {
        if (isWindowsProvider()) {
            return pickDirectorySwing(label, parentDir)
        }

        when (val externalResult = linuxExternalDirectoryChooser(label, parentDir)) {
            is ExternalChooserResult.Selected -> return externalResult.path
            is ExternalChooserResult.Cancelled -> return null
            is ExternalChooserResult.NotAvailable -> { /* fallback to Swing */ }
        }

        return pickDirectorySwing(label, parentDir)
    }

    private fun defaultLinuxFileChooser(label: String, parentDir: File, startFile: File): ExternalChooserResult {
        val active = getActiveWindow()
        val initialPath = if (startFile.isFile) startFile.absolutePath else "${parentDir.absolutePath}/"
        try {
            // On Linux / SteamOS KDE: try kdialog first
            try {
                val kdialog = ProcessBuilder(
                    "kdialog",
                    "--title", "Select $label",
                    "--getopenfilename", initialPath
                ).start()
                val exitCode = kdialog.waitFor()
                if (exitCode == 0) {
                    val path = kdialog.inputStream.bufferedReader().readText().trim()
                    if (path.isNotEmpty()) return ExternalChooserResult.Selected(path)
                } else if (exitCode == 1) {
                    return ExternalChooserResult.Cancelled
                }
            } catch (_: Exception) { /* kdialog not found or failed */ }

            // On GNOME / other Linux: try zenity
            try {
                val zenity = ProcessBuilder(
                    "zenity",
                    "--file-selection",
                    "--title", "Select $label",
                    "--filename=$initialPath"
                ).start()
                val exitCode = zenity.waitFor()
                if (exitCode == 0) {
                    val path = zenity.inputStream.bufferedReader().readText().trim()
                    if (path.isNotEmpty()) return ExternalChooserResult.Selected(path)
                } else if (exitCode == 1) {
                    return ExternalChooserResult.Cancelled
                }
            } catch (_: Exception) { /* zenity not found or failed */ }

            return ExternalChooserResult.NotAvailable
        } finally {
            restoreFocus(active)
        }
    }

    private fun defaultLinuxDirectoryChooser(label: String, parentDir: File): ExternalChooserResult {
        val active = getActiveWindow()
        try {
            // On Linux / SteamOS KDE: try kdialog first
            try {
                val kdialog = ProcessBuilder(
                    "kdialog",
                    "--title", "Select $label",
                    "--getexistingdirectory", parentDir.absolutePath
                ).start()
                val exitCode = kdialog.waitFor()
                if (exitCode == 0) {
                    val path = kdialog.inputStream.bufferedReader().readText().trim()
                    if (path.isNotEmpty()) return ExternalChooserResult.Selected(path)
                } else if (exitCode == 1) {
                    return ExternalChooserResult.Cancelled
                }
            } catch (_: Exception) { /* kdialog not found or failed */ }

            // On GNOME / other Linux: try zenity
            try {
                val zenity = ProcessBuilder(
                    "zenity",
                    "--file-selection",
                    "--directory",
                    "--title", "Select $label",
                    "--filename=${parentDir.absolutePath}/"
                ).start()
                val exitCode = zenity.waitFor()
                if (exitCode == 0) {
                    val path = zenity.inputStream.bufferedReader().readText().trim()
                    if (path.isNotEmpty()) return ExternalChooserResult.Selected(path)
                } else if (exitCode == 1) {
                    return ExternalChooserResult.Cancelled
                }
            } catch (_: Exception) { /* zenity not found or failed */ }

            return ExternalChooserResult.NotAvailable
        } finally {
            restoreFocus(active)
        }
    }

    fun pickFileAwt(label: String, parentDir: File, startFile: File): String? {
        return try {
            if (!EventQueue.isDispatchThread()) {
                var selected: String? = null
                EventQueue.invokeAndWait {
                    selected = fileChooser(label, parentDir, startFile)
                }
                selected
            } else {
                fileChooser(label, parentDir, startFile)
            }
        } catch (_: Throwable) {
            null
        }
    }

    private fun defaultFileChooser(label: String, parentDir: File, startFile: File): String? {
        val active = getActiveWindow()
        val dialog = when (active) {
            is Frame -> FileDialog(active, "Select $label", FileDialog.LOAD)
            is Dialog -> FileDialog(active, "Select $label", FileDialog.LOAD)
            else -> FileDialog(null as Frame?, "Select $label", FileDialog.LOAD)
        }
        return try {
            dialog.directory = parentDir.absolutePath
            if (startFile.isFile) dialog.file = startFile.name
            dialog.isVisible = true
            if (dialog.file != null) {
                File(dialog.directory, dialog.file).absolutePath
            } else {
                null
            }
        } finally {
            dialog.dispose()
            restoreFocus(active)
        }
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

        val active = getActiveWindow()
        val chooser = JFileChooser(parentDir).apply {
            dialogTitle = "Select $label"
            fileSelectionMode = JFileChooser.DIRECTORIES_ONLY
            isAcceptAllFileFilterUsed = false
        }

        return try {
            val result = chooser.showOpenDialog(active)
            if (result == JFileChooser.APPROVE_OPTION && chooser.selectedFile != null) {
                chooser.selectedFile.absolutePath
            } else {
                null
            }
        } finally {
            restoreFocus(active)
        }
    }

    fun getActiveWindow(): Window? {
        return try {
            val focusManager = KeyboardFocusManager.getCurrentKeyboardFocusManager()
            focusManager.activeWindow
                ?: Window.getWindows().firstOrNull { it.isShowing && it.isFocused }
                ?: Window.getWindows().firstOrNull { it.isShowing }
        } catch (_: Throwable) {
            null
        }
    }

    fun restoreFocus(window: Window?) {
        if (window == null) return
        try {
            window.toFront()
            window.requestFocus()
        } catch (_: Throwable) { /* ignore */ }
        EventQueue.invokeLater {
            try {
                window.toFront()
                window.requestFocus()
            } catch (_: Throwable) { /* ignore */ }
        }
    }
}
