package com.emusync.desktop.picker

import java.io.File
import kotlin.test.assertEquals
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class LinuxFileChooserTest {
    @Test
    fun `file selection falls back to zenity and passes paths with spaces as single arguments`(
        @TempDir directory: File
    ) {
        val file = File(directory, "cover with spaces.png").apply { writeText("image") }
        val commands = mutableListOf<List<String>>()
        val chooser = LinuxFileChooser { command ->
            commands += command
            if (command.first() == "kdialog") ExternalChooserResult.NotAvailable
            else ExternalChooserResult.Selected(file.path)
        }

        assertEquals(
            ExternalChooserResult.Selected(file.path),
            chooser.pickFile("Cover Image", directory, file),
        )
        assertEquals(
            listOf(
                "kdialog",
                "--title",
                "Select Cover Image",
                "--getopenfilename",
                file.absolutePath,
            ),
            commands[0],
        )
        assertEquals(
            listOf(
                "zenity",
                "--file-selection",
                "--title",
                "Select Cover Image",
                "--filename=${file.absolutePath}",
            ),
            commands[1],
        )
    }

    @Test
    fun `cancelling the first directory chooser stops without opening another tool`(
        @TempDir directory: File
    ) {
        val commands = mutableListOf<List<String>>()
        val chooser = LinuxFileChooser { command ->
            commands += command
            ExternalChooserResult.Cancelled
        }

        assertEquals(
            ExternalChooserResult.Cancelled,
            chooser.pickDirectory("Save Folder", directory),
        )
        assertEquals(
            listOf(
                listOf(
                    "kdialog",
                    "--title",
                    "Select Save Folder",
                    "--getexistingdirectory",
                    directory.absolutePath,
                )
            ),
            commands,
        )
    }

    @Test
    fun `directory fallback requests directory mode with the initial directory`(
        @TempDir directory: File
    ) {
        val commands = mutableListOf<List<String>>()
        val chooser = LinuxFileChooser { command ->
            commands += command
            if (command.first() == "kdialog") ExternalChooserResult.NotAvailable
            else ExternalChooserResult.Selected(directory.path)
        }

        assertEquals(
            ExternalChooserResult.Selected(directory.path),
            chooser.pickDirectory("Saves", directory),
        )
        assertEquals(
            listOf(
                "zenity",
                "--file-selection",
                "--directory",
                "--title",
                "Select Saves",
                "--filename=${directory.absolutePath}/",
            ),
            commands[1],
        )
    }

    @Test
    fun `unavailable tools return control to the native fallback`(@TempDir directory: File) {
        val commands = mutableListOf<List<String>>()
        val chooser = LinuxFileChooser { command ->
            commands += command
            ExternalChooserResult.NotAvailable
        }

        assertEquals(
            ExternalChooserResult.NotAvailable,
            chooser.pickFile("ROM", directory, File(directory, "missing.rom")),
        )
        assertEquals(listOf("kdialog", "zenity"), commands.map { it.first() })
        assertEquals("${directory.absolutePath}/", commands.first().last())
    }
}
