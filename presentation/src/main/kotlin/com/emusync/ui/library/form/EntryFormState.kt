package com.emusync.ui.library.form

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.emusync.domain.execution.ArgumentsCodec
import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetAccess
import com.emusync.domain.library.scopeNewSaveTargets
import com.emusync.domain.sync.remotePathSegments

/** Draft fields and validation for adding or editing a library entry. */
internal class EntryFormState(initialEntry: GameEntry?, private val targets: SaveTargetAccess) {
    var entryType by
        mutableStateOf(if (initialEntry is NativePCGame) EntryType.NATIVE else EntryType.EMULATOR)
    var name by mutableStateOf(initialEntry?.name.orEmpty())
    var group by mutableStateOf(initialEntry?.group.orEmpty())
    val cloudFolder = initialEntry?.cloudFolder.orEmpty()
    var executablePath by
        mutableStateOf(
            when (initialEntry) {
                is EmulatorSystem -> initialEntry.executablePath
                is NativePCGame -> initialEntry.executablePath
                else -> ""
            }
        )
    var arguments by
        mutableStateOf(
            when (initialEntry) {
                is EmulatorSystem -> ArgumentsCodec.format(initialEntry.arguments)
                is NativePCGame -> ArgumentsCodec.format(initialEntry.arguments)
                else -> "{ROM}"
            }
        )
    var romsDirectory by mutableStateOf((initialEntry as? EmulatorSystem)?.romsDirectory.orEmpty())
    var extensions by
        mutableStateOf((initialEntry as? EmulatorSystem)?.extensions?.joinToString(", ").orEmpty())
    var waitForProcess by mutableStateOf((initialEntry as? NativePCGame)?.waitForProcess.orEmpty())
    var fullscreenArgs by
        mutableStateOf((initialEntry as? EmulatorSystem)?.fullscreenArgs.orEmpty())
    val coversDirectory = (initialEntry as? EmulatorSystem)?.coversDirectory.orEmpty()
    var coverPath by mutableStateOf((initialEntry as? NativePCGame)?.coverPath.orEmpty())
    var errorMessage by mutableStateOf<String?>(null)
    val saves = SaveTargetFormState(initialEntry?.let(targets::forEntry).orEmpty())

    fun applyTypeDefaults() {
        if (entryType == EntryType.EMULATOR && arguments.isBlank()) arguments = "{ROM}"
        if (entryType == EntryType.NATIVE && arguments == "{ROM}") arguments = ""
    }

    fun buildEntry(initialEntry: GameEntry?): GameEntry? {
        val rawTargets = saves.buildTargets()
        if (
            rawTargets.any {
                it.cloudPath?.let { path -> remotePathSegments(path) == null } == true
            }
        ) {
            errorMessage = "Cloud paths must be relative and cannot contain '..'."
            return null
        }
        val targets =
            if (initialEntry?.let(targets::forEntry).isNullOrEmpty())
                scopeNewSaveTargets(rawTargets)
            else rawTargets
        val args = ArgumentsCodec.parse(arguments.trim())
        val entry =
            if (name.isBlank() || executablePath.isBlank() || args == null) null
            else
                when (entryType) {
                    EntryType.EMULATOR -> buildEmulator(args, targets)
                    EntryType.NATIVE -> buildNative(args, targets)
                }
        if (entry == null) errorMessage = "Please fill in all required fields."
        return entry?.let { preserveEntryMetadata(it, initialEntry) }
    }

    private fun buildEmulator(args: List<String>, targets: List<SaveTarget>): EmulatorSystem? {
        val extList = extensions.trim().split(",", " ").map { it.trim() }.filter { it.isNotBlank() }
        return if (romsDirectory.isBlank() || extensions.isBlank() || extList.isEmpty()) null
        else
            EmulatorSystem(
                name = name.trim(),
                executablePath = executablePath.trim(),
                arguments = args,
                romsDirectory = romsDirectory.trim(),
                extensions = extList,
                savePaths = targets.map { it.path },
                saveTargets = targets,
                group = group.trim().ifBlank { null },
                cloudFolder = cloudFolder.trim().ifBlank { null },
                fullscreenArgs = fullscreenArgs.trim().ifBlank { null },
                coversDirectory = coversDirectory.trim().ifBlank { null },
            )
    }

    private fun buildNative(args: List<String>, targets: List<SaveTarget>): NativePCGame? =
        if (targets.isEmpty()) null
        else
            NativePCGame(
                name = name.trim(),
                executablePath = executablePath.trim(),
                arguments = args,
                savePaths = targets.map { it.path },
                saveTargets = targets,
                waitForProcess = waitForProcess.trim().ifBlank { null },
                group = group.trim().ifBlank { null },
                cloudFolder = cloudFolder.trim().ifBlank { null },
                coverPath = coverPath.trim().ifBlank { null },
            )
}

private fun preserveEntryMetadata(entry: GameEntry, initialEntry: GameEntry?): GameEntry =
    when (entry) {
        is EmulatorSystem -> {
            val previous = initialEntry as? EmulatorSystem
            entry.copy(
                driveFileId = initialEntry?.driveFileId,
                steamAppId = initialEntry?.steamAppId,
                steamProcessName = previous?.steamProcessName,
                savePathsByRom = previous?.savePathsByRom.orEmpty(),
                saveTargetsByRom = previous?.saveTargetsByRom.orEmpty(),
                coverPathByRom = previous?.coverPathByRom.orEmpty(),
                titleByRom = previous?.titleByRom.orEmpty(),
            )
        }
        is NativePCGame ->
            entry.copy(
                driveFileId = initialEntry?.driveFileId,
                steamAppId = initialEntry?.steamAppId,
            )
    }
