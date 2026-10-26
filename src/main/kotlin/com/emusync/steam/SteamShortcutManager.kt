package com.emusync.steam

import com.emusync.model.EmulatorSystem
import com.emusync.model.GameEntry
import com.emusync.model.NativePCGame
import com.emusync.util.ArgumentsCodec
import com.emusync.util.shellArgument
import java.io.File
import java.security.MessageDigest
import java.util.Locale
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
 * Linux wrappers and ROM signals use a readable prefix followed by a SHA-256 name identity.
 * Windows shortcuts invoke an encoded PowerShell loader; arguments are stored in JSON.
 * Legacy wrapper paths are retained as forwarders, and ambiguous aliases require a Steam restart.
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
            val regenerated = when (entry) {
                is EmulatorSystem -> buildEmulatorShortcut(entry, appName)
                is NativePCGame -> buildNativeShortcut(entry, appName)
            }
            val updated = duplicate.copy(appName = regenerated.appName, exe = regenerated.exe,
                startDir = regenerated.startDir, launchOptions = regenerated.launchOptions)
            if (updated != duplicate) BinaryVdf.write(vdfFile, existing.map { if (it === duplicate) updated else it })
            if (entry is EmulatorSystem) {
                bridgePreviousName(entry, duplicate, existing)
                installLegacyBridge(entry, existing.map { if (it === duplicate) updated else it })
            }
            return updated
        }

        val newShortcut = when (entry) {
            is EmulatorSystem -> buildEmulatorShortcut(entry, appName)
            is NativePCGame -> buildNativeShortcut(entry, appName)
        }

        BinaryVdf.write(vdfFile, existing + newShortcut)
        if (entry is EmulatorSystem) installLegacyBridge(entry, existing + newShortcut)
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
        val scriptFile = File(launcherDir, "launch_${launcherKey(entry.name)}.sh")
        val signalFile = getSignalFile(entry)

        // Build the argument list, with {ROM} replaced by the signal file read
        val argsPart = entry.arguments.joinToString(" ") { shellArgument(it, expandRom = true) }
        val fullscreenArgs = requireNotNull(ArgumentsCodec.parse(resolveFullscreenArgs(entry))) { "Fullscreen arguments contain unmatched quotes" }
        val fsArgsLine = "FULLSCREEN_ARGS=(" + fullscreenArgs.joinToString(" ") { shellArgument(it) } + ")"

        writeLauncherFile(scriptFile,
            """
            |#!/bin/bash
            |# Auto-generated by EmuSync — do not edit manually.
            |# Launcher for: ${entry.name.replace("\r", " ").replace("\n", " ")}
            |
            |SIGNAL_FILE=${shellArgument(signalFile.absolutePath)}
            |
            |if [ ! -f "${'$'}SIGNAL_FILE" ]; then
            |    echo "EmuSync: No ROM path found. Launch a game from EmuSync first."
            |    exit 1
            |fi
            |
            |ROM_PATH=${'$'}(cat "${'$'}SIGNAL_FILE")
            |
            |if [ ! -e "${'$'}ROM_PATH" ]; then
            |    echo "EmuSync: ROM file not found: ${'$'}ROM_PATH"
            |    exit 1
            |fi
            |
            |# Fullscreen/Big Picture flags to bypass Qt windowed GUI
            |$fsArgsLine
            |
            |echo "EmuSync: Launching with: ${'$'}ROM_PATH"
            |exec ${shellArgument(entry.executablePath)} "${'$'}{FULLSCREEN_ARGS[@]}" $argsPart
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
    fun createWindowsWrapperScript(entry: EmulatorSystem): File =
        WindowsSteamLauncher(getLauncherDir()).create(entry, launcherKey(entry.name), getSignalFile(entry))

    open fun getSignalFile(entry: EmulatorSystem): File =
        File(getLauncherDir(), ".current_rom_${launcherKey(entry.name)}")

    internal fun launcherKey(name: String): String {
        val readable = name.lowercase(Locale.ROOT).replace(Regex("[^a-z0-9]+"), "_").trim('_').take(40).ifEmpty { "emulator" }
        val digest = MessageDigest.getInstance("SHA-256").digest(name.toByteArray(Charsets.UTF_8))
            .joinToString("") { "%02x".format(it) }
        return "$readable-$digest"
    }

    /** Updates registered paths while retaining app IDs, user settings, and old loaded Steam shortcuts. */
    fun migrateLegacyLaunchers(entries: List<GameEntry>): List<SteamShortcut> {
        val file = findShortcutsFile() ?: return emptyList()
        val existing = BinaryVdf.read(file)
        val matches = entries.filterIsInstance<EmulatorSystem>().mapNotNull { entry ->
            existing.find { it.appName == buildAppName(entry) || (entry.steamAppId != null && it.appId == entry.steamAppId) }
                ?.let { entry to it }
        }
        val replacements = matches.associate { (entry, shortcut) ->
            val generated = buildEmulatorShortcut(entry, buildAppName(entry))
            shortcut.appId to shortcut.copy(exe = generated.exe, startDir = generated.startDir, launchOptions = generated.launchOptions)
        }
        val updated = existing.map { replacements[it.appId] ?: it }
        if (updated != existing) {
            file.copyTo(File(file.parentFile, "shortcuts.vdf.bak"), overwrite = true)
            BinaryVdf.write(file, updated)
        }
        for ((entry, previous) in matches) {
            bridgePreviousName(entry, previous, existing)
            installLegacyBridge(entry, existing)
        }
        return updated
    }

    private fun bridgePreviousName(entry: EmulatorSystem, previous: SteamShortcut, shortcuts: List<SteamShortcut>) {
        val previousName = previous.appName.removePrefix("[EmuSync] ")
        if (previousName == entry.name) return
        val oldEntry = entry.copy(name = previousName)
        installLegacyBridge(oldEntry, shortcuts)
        val oldSignal = getSignalFile(oldEntry)
        if (oldSignal.isFile && !getSignalFile(entry).exists()) oldSignal.copyTo(getSignalFile(entry))
        val extension = if (isWindows()) "bat" else "sh"
        val oldScript = File(getLauncherDir(), "launch_${launcherKey(previousName)}.$extension")
        if (!oldScript.exists() && !File(getLauncherDir(), "launch_${previousName.lowercase().replace(Regex("[^a-z0-9]+"), "_")}.$extension").exists()) return
        val contents = if (isWindows()) {
            WindowsSteamLauncher(getLauncherDir()).let { it.batch(it.encodedCommand(launcherKey(entry.name))) }
        } else "#!/bin/bash\nexec ${shellArgument(File(getLauncherDir(), "launch_${launcherKey(entry.name)}.sh").absolutePath)} \"\$@\"\n"
        writeLauncherFile(oldScript, contents)
        if (!isWindows()) oldScript.setExecutable(true)
    }

    private fun installLegacyBridge(entry: EmulatorSystem, shortcuts: List<SteamShortcut>) {
        val oldName = entry.name.lowercase().replace(Regex("[^a-z0-9]+"), "_")
        val conflictingNames = shortcuts.filter { it.appName.startsWith("[EmuSync] ") }
            .map { it.appName.removePrefix("[EmuSync] ") }.distinct()
            .count { it.lowercase().replace(Regex("[^a-z0-9]+"), "_") == oldName }
        val oldSignal = File(getLauncherDir(), ".current_rom_$oldName")
        val newSignal = getSignalFile(entry)
        if (conflictingNames <= 1 && oldSignal.isFile && !newSignal.exists()) oldSignal.copyTo(newSignal)
        val extension = if (isWindows()) "bat" else "sh"
        val previous = File(getLauncherDir(), "launch_$oldName.$extension")
        if (!previous.exists()) return
        val backup = File(previous.parentFile, "${previous.name}.legacy.bak")
        if (!backup.exists()) previous.copyTo(backup)
        val contents = if (conflictingNames > 1) {
            if (isWindows()) "@echo off\r\necho EmuSync: Restart Steam to load the separate emulator launchers.\r\nexit /b 1\r\n"
            else "#!/bin/bash\necho 'EmuSync: Restart Steam to load the separate emulator launchers.' >&2\nexit 1\n"
        } else if (isWindows()) {
            val windows = WindowsSteamLauncher(getLauncherDir())
            windows.batch(windows.encodedCommand(launcherKey(entry.name)))
        } else {
            "#!/bin/bash\nexec ${shellArgument(File(getLauncherDir(), "launch_${launcherKey(entry.name)}.sh").absolutePath)} \"\$@\"\n"
        }
        writeLauncherFile(previous, contents)
        if (!isWindows()) previous.setExecutable(true)
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
     * - Target is Windows PowerShell with an encoded, data-only loader.
     * - The batch wrapper remains available for old Steam processes until restart.
     *
     * On Linux:
     * - Target is the `.sh` script directly
     */
    private fun buildEmulatorShortcut(entry: EmulatorSystem, appName: String): SteamShortcut {
        val launcherScript = createWrapperScript(entry)
        val startDir = resolveStartDir(entry.executablePath)

        return if (isWindows()) {
            val windowsRoot = System.getenv("SystemRoot") ?: "C:\\Windows"
            val powershell = "$windowsRoot\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"
            SteamShortcut(
                appId = generateAppId(),
                appName = appName,
                exe = "\"$powershell\"",
                startDir = startDir,
                launchOptions = "-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand ${WindowsSteamLauncher(getLauncherDir()).encodedCommand(launcherKey(entry.name))}",
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
