package com.emusync.model

import java.io.File
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
enum class SaveTargetType {
    @SerialName("file") FILE,
    @SerialName("directory") DIRECTORY,
    /** The legacy path did not exist during migration, so its type was unknowable. */
    @SerialName("unknown") UNKNOWN,
}

@Serializable
data class SaveTarget(
    val path: String,
    val type: SaveTargetType,
    /**
     * File name or directory prefix within the game's cloud saves; null keeps the legacy layout.
     */
    val cloudPath: String? = null,
)

/** Gives newly configured directory roots distinct, portable cloud paths. */
fun scopeNewSaveTargets(targets: List<SaveTarget>): List<SaveTarget> {
    if (targets.count { it.type == SaveTargetType.DIRECTORY } < 2) return targets
    val used = targets.mapNotNull { it.cloudPath }.toMutableSet()
    var next = 1
    return targets.map { target ->
        if (target.type != SaveTargetType.DIRECTORY || !target.cloudPath.isNullOrBlank()) target
        else {
            var prefix: String
            do {
                prefix = "save-target-${next++}"
            } while (
                used.any { it == prefix || it.startsWith("$prefix/") || prefix.startsWith("$it/") }
            )
            used += prefix
            target.copy(cloudPath = prefix)
        }
    }
}

/** Used only while migrating legacy string-only save paths. */
fun inferLegacySaveTarget(path: String): SaveTarget {
    val file = File(path)
    val type =
        when {
            file.exists() -> if (file.isDirectory) SaveTargetType.DIRECTORY else SaveTargetType.FILE
            else -> SaveTargetType.UNKNOWN
        }
    return SaveTarget(path, type)
}

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

    /** Explicitly typed replacement for [savePaths]. */
    val saveTargets: List<SaveTarget>

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

/**
 * Returns the effective Google Drive folder name used for storing saves. Priority:
 * [GameEntry.cloudFolder] > [GameEntry.group] > [GameEntry.name].
 */
val GameEntry.effectiveCloudFolder: String
    get() =
        cloudFolder?.trim()?.takeIf { it.isNotBlank() }
            ?: group?.trim()?.takeIf { it.isNotBlank() }
            ?: name

/**
 * An emulator-based system.
 *
 * @property name Display name of the emulator system.
 * @property executablePath Absolute path to the emulator's executable.
 * @property arguments List of arguments. Use `{ROM}` as a wildcard for the selected ROM.
 * @property romsDirectory Absolute path to the directory containing ROM files.
 * @property extensions Allowed file extensions (without dots), e.g. ["gba", "gbc"].
 * @property savePaths Legacy global save paths, used when [saveTargets] is empty.
 * @property saveTargets Typed global save locations shared by the system's ROMs.
 * @property savePathsByRom Legacy per-ROM save paths, keyed by filename or filename stem.
 * @property saveTargetsByRom Typed per-ROM save locations, preferred over legacy paths.
 * @property coverPathByRom Custom cover paths keyed by ROM filename.
 * @property titleByRom Custom display titles keyed by ROM filename or matching title.
 * @property group Optional library group and default cloud folder.
 * @property cloudFolder Explicit cloud folder override; blank values fall back to [group] or
 *   [name].
 * @property driveFileId Optional ID of an already-uploaded save in Google Drive.
 * @property steamAppId Optional Steam shortcut ID.
 * @property steamProcessName Process to track when Steam launches the emulator; null uses its
 *   executable name.
 * @property fullscreenArgs Fullscreen flags; null enables detection and an empty string disables
 *   them.
 * @property coversDirectory Optional directory containing cover images.
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
    override val saveTargets: List<SaveTarget> = emptyList(),
    val savePathsByRom: Map<String, List<String>> = emptyMap(),
    val saveTargetsByRom: Map<String, List<SaveTarget>> = emptyMap(),
    /** Optional map of ROM filename to custom cover art image path. */
    val coverPathByRom: Map<String, String> = emptyMap(),
    /** Optional map of ROM filename to custom game display title. */
    val titleByRom: Map<String, String> = emptyMap(),
    override val group: String? = null,
    override val cloudFolder: String? = null,
    override val driveFileId: String? = null,
    override val steamAppId: Int? = null,
    val steamProcessName: String? = null,
    /**
     * Optional flags to force the emulator into Big Picture / fullscreen UI mode. When null,
     * auto-detected from the executable name. Set to an empty string to disable.
     */
    val fullscreenArgs: String? = null,
    /** Optional custom directory for game cover art / boxart images. */
    val coversDirectory: String? = null,
) : GameEntry {
    /**
     * Resolves the effective display title for a ROM.
     * 1. Direct match by exact filename in [titleByRom].
     * 2. Direct match by filename without extension in [titleByRom].
     * 3. Flexible normalized match across devices (e.g. matching "Super_Mario_World.sfc" with
     *    "Super Mario World (USA).sfc").
     * 4. Falls back to [cleanGameTitle] on the provided [fallback].
     */
    fun getEffectiveTitle(romName: String, fallback: String = romName): String {
        val nameWithoutExt =
            if (romName.contains('.')) File(romName).nameWithoutExtension else romName
        return titleByRom[romName]?.trim()?.takeIf { it.isNotBlank() }
            ?: titleByRom[nameWithoutExt]?.trim()?.takeIf { it.isNotBlank() }
            ?: findNormalizedTitle(romName)
            ?: cleanGameTitle(fallback)
    }

    private fun findNormalizedTitle(romName: String): String? {
        val targetKey = normalizeGameKey(romName)
        return if (targetKey.isBlank()) null
        else
            titleByRom.entries
                .firstOrNull { (key, title) ->
                    title.isNotBlank() && normalizeGameKey(key) == targetKey
                }
                ?.value
                ?.trim()
    }
}

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
@Serializable
@SerialName("native")
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

private fun SaveTarget.resolveUnknownTypeIfPresent(): SaveTarget {
    val file = File(path)
    val normalized = if (cloudPath != null && cloudPath.isBlank()) copy(cloudPath = null) else this
    if (!file.exists()) return normalized
    val actualType = if (file.isDirectory) SaveTargetType.DIRECTORY else SaveTargetType.FILE
    val needsTypeUpdate =
        when (type) {
            SaveTargetType.UNKNOWN -> true
            SaveTargetType.FILE -> file.isDirectory
            SaveTargetType.DIRECTORY -> file.isFile
        }
    return if (needsTypeUpdate) {
        normalized.copy(type = actualType)
    } else {
        normalized
    }
}

fun GameEntry.effectiveSaveTargets(): List<SaveTarget> =
    (saveTargets.ifEmpty { savePaths.filter { it.isNotBlank() }.map(::inferLegacySaveTarget) }).map(
        SaveTarget::resolveUnknownTypeIfPresent
    )

fun EmulatorSystem.effectiveSaveTargetsForRom(romName: String): List<SaveTarget> =
    (saveTargetsByRom[romName]?.map(SaveTarget::resolveUnknownTypeIfPresent)
            ?: saveTargetsByRom[romName.substringBeforeLast('.', romName)]?.map(
                SaveTarget::resolveUnknownTypeIfPresent
            )
            ?: savePathsByRom[romName]?.filter { it.isNotBlank() }?.map(::inferLegacySaveTarget)
            ?: savePathsByRom[romName.substringBeforeLast('.', romName)]
                ?.filter { it.isNotBlank() }
                ?.map(::inferLegacySaveTarget))
        .orEmpty()

/** Adds explicit types to legacy paths without changing their path values. */
fun GameEntry.migrateLegacySaveTargets(): GameEntry =
    when (this) {
        is EmulatorSystem ->
            copy(
                saveTargets =
                    (saveTargets.ifEmpty {
                            savePaths.filter { it.isNotBlank() }.map(::inferLegacySaveTarget)
                        })
                        .map(SaveTarget::resolveUnknownTypeIfPresent),
                saveTargetsByRom =
                    saveTargetsByRom.toMutableMap().apply {
                        replaceAll { _, targets ->
                            targets.map(SaveTarget::resolveUnknownTypeIfPresent)
                        }
                        savePathsByRom.forEach { (rom, paths) ->
                            if (rom !in this && paths.isNotEmpty()) {
                                this[rom] =
                                    paths.filter { it.isNotBlank() }.map(::inferLegacySaveTarget)
                            }
                        }
                    },
            )
        is NativePCGame ->
            copy(
                saveTargets =
                    (saveTargets.ifEmpty {
                            savePaths.filter { it.isNotBlank() }.map(::inferLegacySaveTarget)
                        })
                        .map(SaveTarget::resolveUnknownTypeIfPresent)
            )
    }
