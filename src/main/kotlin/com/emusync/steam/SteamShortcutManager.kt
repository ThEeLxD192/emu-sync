package com.emusync.steam

import com.emusync.model.EmulatorSystem
import com.emusync.model.GameEntry
import com.emusync.model.NativePCGame
import java.io.File
import kotlin.random.Random

/**
 * Manages registration of EmuSync entries as Non-Steam Game shortcuts.
 *
 * ## How it works
 *
 * For **emulator systems**, a wrapper script is created that reads the current ROM
 * path from a signal file and launches the emulator with it. This allows EmuSync to
 * change which ROM is launched each time, while the Steam shortcut remains fixed.
 *
 * ```
 * ~/.config/emusync/launchers/
 * ├── launch_game_boy_advance.sh    ← wrapper script for GBA
 * ├── launch_playstation_2.sh       ← wrapper script for PS2
 * └── .current_rom_gba              ← ROM path written before launch
 * ```
 *
 * For **native PC games**, the shortcut points directly to the game executable.
 *
 * ## Usage
 * ```
 * val manager = SteamShortcutManager()
 * val shortcut = manager.registerEntry(myGbaSystem)   // adds to shortcuts.vdf
 * // User restarts Steam, configures controller layout once
 * manager.prepareLaunch(myGbaSystem, romFile)          // writes ROM path
 * // Then launch via steam://rungameid/<appId>
 * ```
 */
open class SteamShortcutManager {

    companion object {
        private val STEAM_BASE = File(System.getProperty("user.home"), ".local/share/Steam/userdata")
        private val LAUNCHER_DIR = File(System.getProperty("user.home"), ".config/emusync/launchers")

        /**
         * Maps executable name patterns to the flags that force the emulator
         * into its fullscreen/Big Picture UI (bypassing the Qt windowed GUI).
         */
        private val FULLSCREEN_FLAGS = mapOf(
            "duckstation"  to "-bigpicture -fullscreen",
            "pcsx2"        to "-bigpicture -fullscreen",
            "dolphin"      to "-b -e",
            "ppsspp"       to "--fullscreen",
            "rpcs3"        to "--fullscreen",
            "eden"         to "-f",
            "yuzu"         to "-f",
            "suyu"         to "-f",
            "citron"       to "-f",
            "ryujinx"      to "--fullscreen",
            "cemu"         to "-f",
            "mgba"         to "-f",
            "melonds"      to "--fullscreen",
        )

        /**
         * Auto-detects the fullscreen flags for an emulator based on its
         * executable filename.  Returns an empty string if no match is found.
         */
        fun detectFullscreenArgs(executablePath: String): String {
            val exeName = File(executablePath).nameWithoutExtension.lowercase()
            return FULLSCREEN_FLAGS.entries
                .firstOrNull { (pattern, _) -> exeName.contains(pattern) }
                ?.value ?: ""
        }

        /**
         * Resolves the fullscreen args for a given entry:
         * - If the user specified a value (even empty string), use that.
         * - Otherwise auto-detect from the executable name.
         */
        fun resolveFullscreenArgs(entry: EmulatorSystem): String {
            return entry.fullscreenArgs ?: detectFullscreenArgs(entry.executablePath)
        }
    }

    /**
     * Finds the `shortcuts.vdf` file for the active Steam user.
     * Skips the "0" directory (offline/anonymous account).
     *
     * @return The shortcuts.vdf [File], or null if Steam is not installed.
     */
    open fun findShortcutsFile(): File? {
        if (!STEAM_BASE.exists()) return null

        return STEAM_BASE.listFiles()
            ?.filter { it.isDirectory && it.name != "0" }
            ?.sortedByDescending { it.lastModified() } // most recently used account first
            ?.map { File(it, "config/shortcuts.vdf") }
            ?.firstOrNull { it.exists() || it.parentFile?.exists() == true }
    }

    /**
     * Loads all existing shortcuts from `shortcuts.vdf`.
     */
    fun loadShortcuts(): List<SteamShortcut> {
        val file = findShortcutsFile() ?: return emptyList()
        return BinaryVdf.read(file)
    }

    /**
     * Checks if a [GameEntry] is already registered as a Steam shortcut.
     * Matches by AppName or AppId.
     */
    fun isRegistered(entry: GameEntry): Boolean {
        val shortcuts = loadShortcuts()
        val targetName = buildAppName(entry)
        val targetAppId = entry.steamAppId
        return shortcuts.any { it.appName == targetName || (targetAppId != null && it.appId == targetAppId) }
    }

    /**
     * Registers a [GameEntry] as a Non-Steam Game shortcut.
     *
     * For [EmulatorSystem]: Creates a wrapper script and points the shortcut to it.
     * For [NativePCGame]: Points the shortcut directly to the executable.
     *
     * @return The created [SteamShortcut] with its generated appId.
     * @throws IllegalStateException if Steam's userdata directory is not found.
     */
    fun registerEntry(entry: GameEntry): SteamShortcut {
        val vdfFile = findShortcutsFile()
            ?: error("Steam userdata directory not found at $STEAM_BASE")

        // Create the file if it doesn't exist yet
        if (!vdfFile.exists()) {
            vdfFile.parentFile?.mkdirs()
            BinaryVdf.write(vdfFile, emptyList())
        }

        // Backup before modifying
        val backupFile = File(vdfFile.parentFile, "shortcuts.vdf.bak")
        vdfFile.copyTo(backupFile, overwrite = true)

        val existing = BinaryVdf.read(vdfFile)

        // Check for existing registration
        val appName = buildAppName(entry)
        val duplicate = existing.find { it.appName == appName || (entry.steamAppId != null && it.appId == entry.steamAppId) }
        if (duplicate != null) {
            // Already registered — regenerate wrapper script in case paths changed
            if (entry is EmulatorSystem) {
                createWrapperScript(entry)
            }
            return duplicate
        }

        val newShortcut = when (entry) {
            is EmulatorSystem -> buildEmulatorShortcut(entry, appName)
            is NativePCGame -> buildNativeShortcut(entry, appName)
        }

        BinaryVdf.write(vdfFile, existing + newShortcut)
        return newShortcut
    }

    /**
     * Removes a shortcut by its appId.
     */
    fun unregisterEntry(appId: Int) {
        val vdfFile = findShortcutsFile() ?: return
        if (!vdfFile.exists()) return

        val backupFile = File(vdfFile.parentFile, "shortcuts.vdf.bak")
        vdfFile.copyTo(backupFile, overwrite = true)

        val existing = BinaryVdf.read(vdfFile)
        val filtered = existing.filter { it.appId != appId }
        BinaryVdf.write(vdfFile, filtered)
    }

    /**
     * Removes a shortcut by its [GameEntry] (matching either AppName or AppId).
     */
    fun unregisterEntry(entry: GameEntry) {
        val vdfFile = findShortcutsFile() ?: return
        if (!vdfFile.exists()) return

        val backupFile = File(vdfFile.parentFile, "shortcuts.vdf.bak")
        vdfFile.copyTo(backupFile, overwrite = true)

        val targetName = buildAppName(entry)
        val targetAppId = entry.steamAppId

        val existing = BinaryVdf.read(vdfFile)
        val filtered = existing.filter {
            it.appName != targetName && (targetAppId == null || it.appId != targetAppId)
        }
        BinaryVdf.write(vdfFile, filtered)
    }

    /**
     * Prepares a launch for an [EmulatorSystem] by writing the ROM path
     * to the signal file that the wrapper script reads.
     *
     * Must be called before launching via `steam://rungameid/<appId>`.
     */
    fun prepareLaunch(entry: EmulatorSystem, romFile: File) {
        val signalFile = getSignalFile(entry)
        signalFile.parentFile?.mkdirs()
        signalFile.writeText(romFile.absolutePath)
    }

    // ── Private helpers ──────────────────────────────────────────────

    /**
     * Builds a display name for the Steam shortcut.
     * Prefixed with "[EmuSync]" to clearly identify managed shortcuts.
     */
    private fun buildAppName(entry: GameEntry): String {
        return "[EmuSync] ${entry.name}"
    }

    /**
     * Creates a wrapper script for an emulator and returns the shortcut.
     *
     * The wrapper script:
     * 1. Reads the ROM path from a signal file
     * 2. Launches the emulator with the ROM as argument
     * 3. Waits for the emulator to exit
     */
    private fun buildEmulatorShortcut(entry: EmulatorSystem, appName: String): SteamShortcut {
        val launcherScript = createWrapperScript(entry)
        val startDir = File(entry.executablePath).parentFile?.absolutePath ?: "."

        return SteamShortcut(
            appId = generateAppId(),
            appName = appName,
            exe = "\"${launcherScript.absolutePath}\"",
            startDir = startDir,
            tags = mapOf("0" to "EmuSync"),
        )
    }

    /**
     * Creates a shortcut for a native PC game (direct executable).
     */
    private fun buildNativeShortcut(entry: NativePCGame, appName: String): SteamShortcut {
        val startDir = File(entry.executablePath).parentFile?.absolutePath ?: "."
        val baseArgs = entry.arguments.joinToString(" ")
        
        val launchOptions = if (baseArgs.isNotBlank()) "%command% $baseArgs" else ""

        return SteamShortcut(
            appId = generateAppId(),
            appName = appName,
            exe = "\"${entry.executablePath}\"",
            startDir = startDir,
            launchOptions = launchOptions,
            tags = mapOf("0" to "EmuSync"),
        )
    }

    /**
     * Creates a wrapper shell script for an emulator system.
     *
     * The script reads the ROM path from a signal file and launches
     * the emulator with the correct arguments, replacing {ROM} with
     * the actual ROM path.
     */
    private fun createWrapperScript(entry: EmulatorSystem): File {
        LAUNCHER_DIR.mkdirs()
        val safeName = entry.name.lowercase().replace(Regex("[^a-z0-9]+"), "_")
        val scriptFile = File(LAUNCHER_DIR, "launch_$safeName.sh")
        val signalFile = getSignalFile(entry)

        // Build the argument list, with {ROM} replaced by the signal file read
        val argsPart = entry.arguments.joinToString(" ") { arg ->
            if (arg == "{ROM}") "\"\$ROM_PATH\"" else "\"$arg\""
        }

        scriptFile.writeText(
            """
            |#!/bin/bash
            |# Auto-generated by EmuSync — do not edit manually.
            |# Launcher for: ${entry.name}
            |
            |SIGNAL_FILE="${signalFile.absolutePath}"
            |
            |if [ ! -f "${'$'}SIGNAL_FILE" ]; then
            |    echo "EmuSync: No ROM path found. Launch a game from EmuSync first."
            |    exit 1
            |fi
            |
            |ROM_PATH=${'$'}(cat "${'$'}SIGNAL_FILE")
            |
            |if [ ! -f "${'$'}ROM_PATH" ]; then
            |    echo "EmuSync: ROM file not found: ${'$'}ROM_PATH"
            |    exit 1
            |fi
            |
            |# Fullscreen/Big Picture flags to bypass Qt windowed GUI
            |FULLSCREEN_ARGS="${resolveFullscreenArgs(entry)}"
            |
            |echo "EmuSync: Launching ${entry.name} with: ${'$'}ROM_PATH"
            |exec "${entry.executablePath}" ${'$'}FULLSCREEN_ARGS $argsPart
            """.trimMargin() + "\n"
        )

        scriptFile.setExecutable(true)
        return scriptFile
    }

    /**
     * Returns the signal file path for a given emulator system.
     * This file stores the ROM path to launch next.
     */
    private fun getSignalFile(entry: EmulatorSystem): File {
        val safeName = entry.name.lowercase().replace(Regex("[^a-z0-9]+"), "_")
        return File(LAUNCHER_DIR, ".current_rom_$safeName")
    }

    /**
     * Generates a random appId for a new shortcut.
     * Uses a high range to avoid collisions with real Steam app IDs.
     */
    private fun generateAppId(): Int {
        // Generate a random ID in a high range (negative in signed int = high unsigned)
        // This matches how Steam assigns IDs to non-Steam games.
        return Random.nextInt(Int.MIN_VALUE, -1000)
    }
}
