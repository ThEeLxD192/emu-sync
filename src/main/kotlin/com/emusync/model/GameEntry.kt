package com.emusync.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * Sealed interface representing a game entry in the configuration.
 *
 * Two concrete types:
 * - [EmulatorSystem]: A system whose ROMs live in a directory and are launched via a base command.
 * - [NativePCGame]: A standalone PC game/port with a direct executable and explicit save path.
 *
 * The `type` discriminator in JSON ("emulator" or "native") controls which subtype is deserialized.
 */
@Serializable
sealed interface GameEntry {
    /** Display name shown in the UI (e.g. "Game Boy Advance" or "Spelunky Classic"). */
    val name: String

    // Note: savePaths acts as a global fallback for EmulatorSystem, 
    // overridden by savePathsByRom for explicit games.
    val savePaths: List<String>

    /**
     * Optional Google Drive file ID for an already-uploaded save.
     * When null, the first sync will create the file on Drive.
     */
    val driveFileId: String?

    /**
     * Optional Steam Non-Steam Game shortcut AppID.
     */
    val steamAppId: Int?
}

/**
 * An emulator-based system.
 *
 * @property executablePath Absolute path to the emulator's executable.
 * @property arguments      List of arguments. Use `{ROM}` as a wildcard for the selected ROM.
 * @property romsDirectory  Absolute path to the directory containing ROM files.
 * @property extensions     Allowed file extensions (without dots), e.g. ["gba", "gbc"].
 */
@Serializable
@SerialName("emulator")
data class EmulatorSystem(
    override val name: String,
    val executablePath: String,
    val arguments: List<String> = listOf("{ROM}"),
    val romsDirectory: String,
    val extensions: List<String>,
    override val savePaths: List<String> = emptyList(),
    val savePathsByRom: Map<String, List<String>> = emptyMap(),
    override val driveFileId: String? = null,
    override val steamAppId: Int? = null,
    val steamProcessName: String? = null,
    /** Optional flags to force the emulator into Big Picture / fullscreen UI mode.
     *  When null, auto-detected from the executable name.
     *  Set to an empty string to disable. */
    val fullscreenArgs: String? = null,
    /** Optional custom directory for game cover art / boxart images. */
    val coversDirectory: String? = null,
) : GameEntry

/**
 * A native PC game or decompiled port.
 *
 * @property executablePath Absolute path to the game's executable.
 * @property arguments      Optional CLI arguments passed to the executable.
 */
@Serializable
@SerialName("native")
data class NativePCGame(
    override val name: String,
    val executablePath: String,
    val arguments: List<String> = emptyList(),
    override val savePaths: List<String> = emptyList(),
    val waitForProcess: String? = null,
    override val driveFileId: String? = null,
    override val steamAppId: Int? = null,
    /** Optional custom path to a cover art image for this game. */
    val coverPath: String? = null,
) : GameEntry
