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
 * Linux:
 * ```
 * ~/.config/emusync/launchers/
 * ├── launch_game_boy_advance.sh    ← wrapper script for GBA
 * ├── launch_playstation_2.sh       ← wrapper script for PS2
 * └── .current_rom_gba              ← ROM path written before launch
 * ```
 *
 * Windows:
 * ```
 * %APPDATA%\emusync\launchers\
 * ├── launch_game_boy_advance.bat   ← batch wrapper script with start /wait
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
open class SteamShortcutManager(
    private val customSteamUserdataDir: File? = null,
    private val customLauncherDir: File? = null,
    private val registryQuery: (key: String, valueName: String) -> String? = ::defaultRegistryQuery,
) {

    companion object {
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
            "retroarch"    to "-f",
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
         * executable filename. Returns an empty string if no match is found.
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

        /**
         * Parses the standard output of Windows `reg query <key> /v <valueName>`.
         * Returns the extracted string value, or null if not found or invalid.
         */
        fun parseRegistryOutput(output: String, valueName: String): String? {
            val pattern = Regex(
                """(?m)^\s*${Regex.escape(valueName)}\s+REG_(?:SZ|EXPAND_SZ)\s+(.+)$""",
                RegexOption.IGNORE_CASE
            )
            val match = pattern.find(output) ?: return null
            return match.groupValues[1].trim().takeIf { it.isNotEmpty() }
        }

        /**
         * Default query implementation for Windows registry using `reg query`.
         */
        fun defaultRegistryQuery(key: String, valueName: String): String? {
            if (!System.getProperty("os.name", "").lowercase().contains("windows")) {
                return null
            }
            return try {
                val process = ProcessBuilder("reg", "query", key, "/v", valueName)
                    .redirectErrorStream(true)
                    .start()
                val output = process.inputStream.bufferedReader().readText()
                process.waitFor()
                if (process.exitValue() == 0) {
                    parseRegistryOutput(output, valueName)
                } else {
                    null
                }
            } catch (_: Exception) {
                null
            }
        }
    }

    /**
     * Indicates whether the host OS is Windows.
     * Can be overridden in tests to simulate Windows environment.
     */
    open fun isWindows(): Boolean =
        System.getProperty("os.name", "").lowercase().contains("windows")

    /**
     * Resolves the Steam `userdata` directory.
     * On Windows, queries the registry and candidate install directories.
     * On Linux, checks standard and Flatpak Steam locations.
     */
    open fun getSteamUserdataDir(): File? {
        if (customSteamUserdataDir != null) return customSteamUserdataDir
        return if (isWindows()) {
            findWindowsSteamUserdataDir()
        } else {
            findLinuxSteamUserdataDir()
        }
    }

    /**
     * Locates the Steam `userdata` directory on Windows.
     */
    open fun findWindowsSteamUserdataDir(): File? {
        // 1. Check registry: HKCU\Software\Valve\Steam -> SteamPath
        val hkcuPath = registryQuery("HKCU\\Software\\Valve\\Steam", "SteamPath")
        if (!hkcuPath.isNullOrBlank()) {
            val dir = File(hkcuPath.replace('/', File.separatorChar), "userdata")
            if (dir.exists() && dir.isDirectory) return dir
        }

        // 2. Check registry: HKLM\SOFTWARE\WOW6432Node\Valve\Steam -> InstallPath
        val hklmPath = registryQuery("HKLM\\SOFTWARE\\WOW6432Node\\Valve\\Steam", "InstallPath")
            ?: registryQuery("HKLM\\SOFTWARE\\Valve\\Steam", "InstallPath")
        if (!hklmPath.isNullOrBlank()) {
            val dir = File(hklmPath.replace('/', File.separatorChar), "userdata")
            if (dir.exists() && dir.isDirectory) return dir
        }

        // 3. Fallback common paths on Windows
        val candidates = listOfNotNull(
            System.getenv("ProgramFiles(x86)")?.let { File(it, "Steam/userdata") },
            System.getenv("ProgramFiles")?.let { File(it, "Steam/userdata") },
            File("C:\\Program Files (x86)\\Steam\\userdata"),
            File("C:\\Program Files\\Steam\\userdata"),
            File("C:\\Steam\\userdata"),
            File("D:\\Steam\\userdata"),
        )
        val existingCandidate = candidates.firstOrNull { it.exists() && it.isDirectory }
        if (existingCandidate != null) return existingCandidate

        // If userdata folder does not exist yet but Steam root exists, return userdata path inside it
        if (!hkcuPath.isNullOrBlank()) {
            val root = File(hkcuPath.replace('/', File.separatorChar))
            if (root.exists()) return File(root, "userdata")
        }
        if (!hklmPath.isNullOrBlank()) {
            val root = File(hklmPath.replace('/', File.separatorChar))
            if (root.exists()) return File(root, "userdata")
        }

        return candidates.firstOrNull()
    }

    /**
     * Locates the Steam `userdata` directory on Linux (standard or Flatpak).
     */
    open fun findLinuxSteamUserdataDir(): File? {
        val userHome = System.getProperty("user.home", "")
        val candidates = listOfNotNull(
            File(userHome, ".local/share/Steam/userdata"),
            File(userHome, ".steam/steam/userdata"),
            File(userHome, ".steam/root/userdata"),
            File(userHome, ".var/app/com.valvesoftware.Steam/.local/share/Steam/userdata"),
            File(userHome, ".var/app/com.valvesoftware.Steam/.steam/steam/userdata"),
        )
        return candidates.firstOrNull { it.exists() && it.isDirectory }
            ?: candidates.first()
    }

    /**
     * Returns the directory where generated emulator launcher scripts are stored.
     * Linux: ~/.config/emusync/launchers
     * Windows: %APPDATA%\emusync\launchers
     */
    open fun getLauncherDir(): File {
        if (customLauncherDir != null) return customLauncherDir
        return if (isWindows()) {
            val appData = System.getenv("APPDATA")
            if (!appData.isNullOrBlank()) {
                File(appData, "emusync/launchers")
            } else {
                File(System.getProperty("user.home"), "AppData/Roaming/emusync/launchers")
            }
        } else {
            val xdgConfig = System.getenv("XDG_CONFIG_HOME")
            if (!xdgConfig.isNullOrBlank()) {
                File(xdgConfig, "emusync/launchers")
            } else {
                File(System.getProperty("user.home"), ".config/emusync/launchers")
            }
        }
    }

    /**
     * Finds the `shortcuts.vdf` file for the active Steam user.
     * Skips the "0" directory (offline/anonymous account).
     *
     * @return The shortcuts.vdf [File], or null if Steam is not installed.
     */
    open fun findShortcutsFile(): File? {
        val base = getSteamUserdataDir() ?: return null
        if (!base.exists() || !base.isDirectory) return null

        val accounts = base.listFiles()
            ?.filter { it.isDirectory && it.name != "0" }
            ?.sortedByDescending { it.lastModified() } // most recently used account first
            ?: return null

        return accounts.map { File(it, "config/shortcuts.vdf") }
            .firstOrNull { it.exists() || it.parentFile?.exists() == true }
            ?: accounts.firstOrNull()?.let { File(it, "config/shortcuts.vdf") }
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
            ?: error("Steam userdata directory not found (searched ${getSteamUserdataDir()?.absolutePath ?: "default locations"})")

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

    /**
     * Creates a wrapper script appropriate for the current OS.
     */
    fun createWrapperScript(entry: EmulatorSystem): File {
        return if (isWindows()) {
            createWindowsWrapperScript(entry)
        } else {
            createLinuxWrapperScript(entry)
        }
    }

    /**
     * Creates a wrapper shell script (`.sh`) for an emulator system on Linux.
     */
    fun createLinuxWrapperScript(entry: EmulatorSystem): File {
        val launcherDir = getLauncherDir()
        launcherDir.mkdirs()
        val safeName = entry.name.lowercase().replace(Regex("[^a-z0-9]+"), "_")
        val scriptFile = File(launcherDir, "launch_$safeName.sh")
        val signalFile = getSignalFile(entry)

        // Build the argument list, with {ROM} replaced by the signal file read
        val argsPart = entry.arguments.joinToString(" ") { arg ->
            if (arg == "{ROM}") "\"\$ROM_PATH\"" else "\"$arg\""
        }

        val fsArgs = resolveFullscreenArgs(entry)
        val fsArgsLine = if (fsArgs.isNotBlank()) "FULLSCREEN_ARGS=\"$fsArgs\"" else "FULLSCREEN_ARGS=\"\""

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
            |$fsArgsLine
            |
            |echo "EmuSync: Launching ${entry.name} with: ${'$'}ROM_PATH"
            |exec "${entry.executablePath}" ${'$'}FULLSCREEN_ARGS $argsPart
            """.trimMargin() + "\n"
        )

        scriptFile.setExecutable(true)
        return scriptFile
    }

    /**
     * Creates a wrapper batch script (`.bat`) for an emulator system on Windows.
     * Uses `start "" /wait` to ensure the batch interpreter waits for GUI emulator
     * processes to exit, allowing Steam to track playtime accurately.
     */
    fun createWindowsWrapperScript(entry: EmulatorSystem): File {
        val launcherDir = getLauncherDir()
        launcherDir.mkdirs()
        val safeName = entry.name.lowercase().replace(Regex("[^a-z0-9]+"), "_")
        val scriptFile = File(launcherDir, "launch_$safeName.bat")
        val signalFile = getSignalFile(entry)

        val argsPart = entry.arguments.joinToString(" ") { arg ->
            if (arg == "{ROM}") "\"%ROM_PATH%\"" else "\"$arg\""
        }

        val fsArgs = resolveFullscreenArgs(entry)
        val combinedArgs = listOf(fsArgs, argsPart).filter { it.isNotBlank() }.joinToString(" ")
        val launchLine = if (combinedArgs.isNotBlank()) {
            "start \"\" /wait \"${entry.executablePath}\" $combinedArgs"
        } else {
            "start \"\" /wait \"${entry.executablePath}\""
        }

        val scriptContent = """
            |@echo off
            |:: Auto-generated by EmuSync — do not edit manually.
            |:: Launcher for: ${entry.name}
            |
            |set "SIGNAL_FILE=${signalFile.absolutePath}"
            |
            |if not exist "%SIGNAL_FILE%" (
            |    echo EmuSync: No ROM path found. Launch a game from EmuSync first.
            |    exit /b 1
            |)
            |
            |set /p ROM_PATH=<"%SIGNAL_FILE%"
            |
            |if not exist "%ROM_PATH%" (
            |    echo EmuSync: ROM file not found: %ROM_PATH%
            |    exit /b 1
            |)
            |
            |echo EmuSync: Launching ${entry.name} with: %ROM_PATH%
            |$launchLine
            |exit /b %ERRORLEVEL%
        """.trimMargin().replace("\n", "\r\n") + "\r\n"

        scriptFile.writeText(scriptContent)
        return scriptFile
    }

    /**
     * Returns the signal file path for a given emulator system.
     * This file stores the ROM path to launch next.
     */
    open fun getSignalFile(entry: EmulatorSystem): File {
        val safeName = entry.name.lowercase().replace(Regex("[^a-z0-9]+"), "_")
        return File(getLauncherDir(), ".current_rom_$safeName")
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
     * Resolves the start directory for an executable across platforms.
     */
    fun resolveStartDir(executablePath: String): String {
        val normalized = executablePath.replace('\\', '/')
        val lastSlash = normalized.lastIndexOf('/')
        if (lastSlash < 0) return "."
        val parent = normalized.substring(0, lastSlash)
        if (parent.isEmpty()) return "/"
        return if (isWindows()) parent.replace('/', '\\') else parent
    }

    /**
     * Creates a wrapper script for an emulator and returns the shortcut.
     *
     * On Windows:
     * - Target is `cmd.exe`
     * - Launch options run the batch script via `/c ""`
     *
     * On Linux:
     * - Target is the `.sh` script directly
     */
    private fun buildEmulatorShortcut(entry: EmulatorSystem, appName: String): SteamShortcut {
        val launcherScript = createWrapperScript(entry)
        val startDir = resolveStartDir(entry.executablePath)

        return if (isWindows()) {
            val comspec = System.getenv("COMSPEC") ?: "C:\\Windows\\System32\\cmd.exe"
            SteamShortcut(
                appId = generateAppId(),
                appName = appName,
                exe = "\"$comspec\"",
                startDir = startDir,
                launchOptions = "/c \"\"${launcherScript.absolutePath}\"\"",
                tags = mapOf("0" to "EmuSync"),
            )
        } else {
            SteamShortcut(
                appId = generateAppId(),
                appName = appName,
                exe = "\"${launcherScript.absolutePath}\"",
                startDir = startDir,
                tags = mapOf("0" to "EmuSync"),
            )
        }
    }

    /**
     * Creates a shortcut for a native PC game (direct executable).
     */
    private fun buildNativeShortcut(entry: NativePCGame, appName: String): SteamShortcut {
        val startDir = resolveStartDir(entry.executablePath)
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
     * Generates a random appId for a new shortcut.
     * Uses a high range to avoid collisions with real Steam app IDs.
     */
    private fun generateAppId(): Int {
        // Generate a random ID in a high range (negative in signed int = high unsigned)
        // This matches how Steam assigns IDs to non-Steam games.
        return Random.nextInt(Int.MIN_VALUE, -1000)
    }
}
