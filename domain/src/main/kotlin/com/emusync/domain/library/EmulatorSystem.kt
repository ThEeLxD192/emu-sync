package com.emusync.domain.library

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
        val nameWithoutExt = romName.substringBeforeLast('.', romName)
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
