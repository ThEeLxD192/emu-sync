package com.emusync.ui.library.form

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.emusync.application.library.gameSpecificTargets
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetAccess
import com.emusync.domain.library.SaveTargetType
import com.emusync.domain.library.scopeNewSaveTargets
import com.emusync.ui.library.GameItem

/** Draft overrides and restore choices for one game, without emulator-wide save fallbacks. */
internal class GameEditFormState(
    private val game: GameItem,
    private val targets: SaveTargetAccess,
) {
    var customTitle by mutableStateOf(game.effectiveTitle)
    var coverPath by mutableStateOf(game.effectiveCoverPath.orEmpty())
    var restoreDestinationFolder by mutableStateOf("")
    var restoreWholeFolder by mutableStateOf(false)
    val saves =
        SaveTargetFormState(targets.gameSpecificTargets(game.entry, game.romFile?.name), targets)

    fun restorePaths(cloudFileNames: List<String>): List<String> {
        val destination = restoreDestinationFolder.trim()
        return when {
            cloudFileNames.isEmpty() || destination.isBlank() -> emptyList()
            restoreWholeFolder -> listOf(destination)
            else -> targets.forRestore(destination, cloudFileNames).map { it.path }.distinct()
        }
    }

    fun canSave(cloudFileNames: List<String>, selectedRestorePaths: List<String>): Boolean =
        (cloudFileNames.isEmpty() || selectedRestorePaths.isNotEmpty()) && saves.cloudPathsAreValid

    fun result(game: GameItem, cloudFileNames: List<String>): GameEditResult {
        val rawTargets =
            when {
                cloudFileNames.isEmpty() -> saves.buildTargets()
                restoreWholeFolder ->
                    listOf(SaveTarget(restoreDestinationFolder.trim(), SaveTargetType.DIRECTORY))
                else -> targets.forRestore(restoreDestinationFolder.trim(), cloudFileNames)
            }
        val targets =
            if (
                cloudFileNames.isEmpty() &&
                    targets.gameSpecificTargets(game.entry, game.romFile?.name).isEmpty()
            )
                scopeNewSaveTargets(rawTargets)
            else rawTargets
        return GameEditResult(
            customTitle.trim().ifBlank { null },
            targets,
            coverPath.trim().ifBlank { null },
        )
    }
}
