package com.emusync.ui.dialogs

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetType
import com.emusync.domain.library.scopeNewSaveTargets
import com.emusync.drive.resolveCloudRestorePaths
import com.emusync.drive.resolveCloudRestoreTargets
import com.emusync.ui.GameItem

internal data class GameEditResult(
    val title: String?,
    val targets: List<SaveTarget>,
    val coverPath: String?,
)

/** Draft overrides and restore choices for one game, without emulator-wide save fallbacks. */
internal class GameEditFormState(game: GameItem) {
    var customTitle by mutableStateOf(game.effectiveTitle)
    var coverPath by mutableStateOf(game.effectiveCoverPath.orEmpty())
    var restoreDestinationFolder by mutableStateOf("")
    var restoreWholeFolder by mutableStateOf(false)
    val saves = SaveTargetFormState(game.gameSpecificSaveTargets)

    fun restorePaths(cloudFileNames: List<String>): List<String> {
        val destination = restoreDestinationFolder.trim()
        return when {
            cloudFileNames.isEmpty() || destination.isBlank() -> emptyList()
            restoreWholeFolder -> listOf(destination)
            else -> resolveCloudRestorePaths(destination, cloudFileNames)
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
                else -> resolveCloudRestoreTargets(restoreDestinationFolder.trim(), cloudFileNames)
            }
        val targets =
            if (cloudFileNames.isEmpty() && game.gameSpecificSaveTargets.isEmpty())
                scopeNewSaveTargets(rawTargets)
            else rawTargets
        return GameEditResult(
            customTitle.trim().ifBlank { null },
            targets,
            coverPath.trim().ifBlank { null },
        )
    }
}
