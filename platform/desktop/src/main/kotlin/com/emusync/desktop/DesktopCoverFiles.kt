package com.emusync.desktop

import com.emusync.domain.library.CoverFiles
import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.cleanGameTitle
import java.io.File

class DesktopCoverFiles : CoverFiles {
    override fun find(name: String, entry: GameEntry, romPath: String?): String? =
        CoverSearch(name, entry, romPath).find()?.path
}

/** Preserves custom, adjacent and configured-folder priority, including legacy name variants. */
private class CoverSearch(
    private val name: String,
    private val entry: GameEntry,
    romPath: String?,
) {
    private val romFile = romPath?.let(::File)
    private val baseName = romFile?.nameWithoutExtension ?: name
    private val cleanName = cleanGameTitle(baseName)
    private val spaceName = baseName.replace('_', ' ').trim()
    private val romDirectory =
        romFile?.parentFile ?: (entry as? NativePCGame)?.let { File(it.executablePath).parentFile }
    private val extensions = listOf("png", "jpg", "jpeg", "webp")

    fun find(): File? {
        val customPath =
            when (entry) {
                is EmulatorSystem -> romFile?.name?.let(entry.coverPathByRom::get)
                is NativePCGame -> entry.coverPath
            }
        return customPath?.takeIf { it.isNotBlank() }?.let(::File)?.takeIf { it.isFile }
            ?: adjacentCover()
            ?: when (entry) {
                is EmulatorSystem -> emulatorCover(entry)
                is NativePCGame -> nativeCover(entry)
            }
    }

    private fun adjacentCover(): File? {
        val directory = romDirectory?.takeIf { it.exists() } ?: return null
        val title =
            if (entry is EmulatorSystem) entry.getEffectiveTitle(romFile?.name ?: name, name)
            else entry.name
        val names = listOfNotNull(baseName, title, cleanName, spaceName, romFile?.name).distinct()
        return candidates(directory, names).firstOrNull { it.isFile }
            ?: directory.listFiles().orEmpty().firstOrNull {
                isImage(it) && it.nameWithoutExtension.equals(baseName, ignoreCase = true)
            }
    }

    private fun emulatorCover(system: EmulatorSystem): File? {
        val directories = emulatorDirectories(system)
        val names = emulatorNames()
        return directories
            .asSequence()
            .mapNotNull { directory ->
                candidates(directory, names).firstOrNull { it.exists() }
                    ?: directory.listFiles().orEmpty().firstOrNull { file ->
                        isImage(file) &&
                            listOf(baseName, cleanName, spaceName).any {
                                file.nameWithoutExtension.equals(it, ignoreCase = true)
                            }
                    }
            }
            .firstOrNull()
    }

    private fun emulatorDirectories(system: EmulatorSystem): List<File> {
        val configured =
            system.coversDirectory?.takeIf { it.isNotBlank() }?.let(::File)?.takeIf { it.exists() }
        val adjacent =
            romDirectory
                ?.takeIf { it.exists() }
                ?.let { directory ->
                    listOf(
                        File(directory, "covers"),
                        File(directory, "boxart"),
                        File(directory, "images"),
                    )
                }
                .orEmpty()
        return listOfNotNull(configured) + adjacent
    }

    private fun emulatorNames(): List<String> =
        listOfNotNull(
            baseName,
            cleanName.takeIf { it.isNotBlank() && it != baseName },
            spaceName.takeIf { it.isNotBlank() && it != baseName && it != cleanName },
            romFile?.name,
        )

    private fun nativeCover(game: NativePCGame): File? {
        val directory = File(game.executablePath).parentFile?.takeIf { it.exists() } ?: return null
        return candidates(directory, listOf(name, cleanName, "cover", "boxart")).firstOrNull {
            it.exists()
        }
    }

    private fun candidates(directory: File, names: List<String>): Sequence<File> =
        names.asSequence().flatMap { candidate ->
            extensions.asSequence().flatMap { extension ->
                sequenceOf(
                    File(directory, "$candidate.$extension"),
                    File(directory, "$candidate.${extension.uppercase()}"),
                )
            }
        }

    private fun isImage(file: File): Boolean =
        file.isFile && extensions.any { file.extension.equals(it, ignoreCase = true) }
}
