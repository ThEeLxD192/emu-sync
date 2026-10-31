package com.emusync.domain.library

/**
 * A native PC game or decompiled port.
 *
 * @property name Display name of the game.
 * @property executablePath Absolute path to the game's executable.
 * @property arguments Optional CLI arguments passed to the executable.
 * @property savePaths Legacy save paths, used when [saveTargets] is empty.
 * @property saveTargets Typed save locations for the game.
 * @property waitForProcess Optional process name to track after the launcher exits.
 * @property group Optional library group and default cloud folder.
 * @property cloudFolder Explicit cloud folder override; blank values fall back to [group] or
 *   [name].
 * @property driveFileId Optional ID of an already-uploaded save in Google Drive.
 * @property steamAppId Optional Steam shortcut ID.
 * @property coverPath Optional custom cover image path.
 */
data class NativePCGame(
    override val name: String,
    val executablePath: String,
    val arguments: List<String> = emptyList(),
    override val savePaths: List<String> = emptyList(),
    override val saveTargets: List<SaveTarget> = emptyList(),
    val waitForProcess: String? = null,
    override val group: String? = null,
    override val cloudFolder: String? = null,
    override val driveFileId: String? = null,
    override val steamAppId: Int? = null,
    /** Optional custom path to a cover art image for this game. */
    val coverPath: String? = null,
) : GameEntry
