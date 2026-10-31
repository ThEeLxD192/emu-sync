package com.emusync.config.dto

import kotlinx.serialization.Serializable

/**
 * Sealed interface representing a game entry in the configuration.
 *
 * Two concrete types:
 * - [EmulatorSystemDto]: A system whose ROMs live in a directory and are launched via a base
 *   command.
 * - [NativePCGameDto]: A standalone PC game/port with a direct executable and explicit save path.
 *
 * The `type` discriminator in JSON ("emulator" or "native") controls which subtype is deserialized.
 */
@Serializable
internal sealed interface GameEntryDto {
    /** Display name shown in the UI (e.g. "Game Boy Advance" or "Spelunky Classic"). */
    val name: String

    // Note: savePaths acts as a global fallback for EmulatorSystemDto,
    // overridden by savePathsByRom for explicit games.
    val savePaths: List<String>

    /** Explicitly typed replacement for [savePaths]. */
    val saveTargets: List<SaveTargetDto>

    /**
     * Optional folder/category name to group emulators together in the UI and share a unified cloud
     * saves folder (e.g. "Switch", "PlayStation").
     */
    val group: String?

    /**
     * Optional custom Google Drive folder name. When null or blank, defaults to [group] (if set) or
     * [name].
     */
    val cloudFolder: String?

    /**
     * Optional Google Drive file ID for an already-uploaded save. When null, the first sync will
     * create the file on Drive.
     */
    val driveFileId: String?

    /** Optional Steam Non-Steam Game shortcut AppID. */
    val steamAppId: Int?
}
