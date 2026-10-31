package com.emusync.desktop.picker

import java.io.File

/** Tries desktop tools in order, falling back only when a tool is unavailable or fails. */
internal class LinuxFileChooser(
    private val runCommand: (List<String>) -> ExternalChooserResult = ::runExternalChooser
) {
    fun pickFile(label: String, parent: File, start: File): ExternalChooserResult {
        val initialPath = if (start.isFile) start.absolutePath else "${parent.absolutePath}/"
        return choose(
            listOf("kdialog", "--title", "Select $label", "--getopenfilename", initialPath),
            listOf(
                "zenity",
                "--file-selection",
                "--title",
                "Select $label",
                "--filename=$initialPath",
            ),
        )
    }

    fun pickDirectory(label: String, parent: File): ExternalChooserResult =
        choose(
            listOf(
                "kdialog",
                "--title",
                "Select $label",
                "--getexistingdirectory",
                parent.absolutePath,
            ),
            listOf(
                "zenity",
                "--file-selection",
                "--directory",
                "--title",
                "Select $label",
                "--filename=${parent.absolutePath}/",
            ),
        )

    private fun choose(vararg commands: List<String>): ExternalChooserResult {
        var result: ExternalChooserResult = ExternalChooserResult.NotAvailable
        for (command in commands) {
            result = runCommand(command)
            if (result != ExternalChooserResult.NotAvailable) break
        }
        return result
    }
}

private fun runExternalChooser(command: List<String>): ExternalChooserResult =
    try {
        val process = ProcessBuilder(command).start()
        when (process.waitFor()) {
            0 ->
                process.inputStream
                    .bufferedReader()
                    .readText()
                    .trim()
                    .takeIf { it.isNotEmpty() }
                    ?.let(ExternalChooserResult::Selected) ?: ExternalChooserResult.NotAvailable
            1 -> ExternalChooserResult.Cancelled
            else -> ExternalChooserResult.NotAvailable
        }
    } catch (_: Exception) {
        ExternalChooserResult.NotAvailable
    }
