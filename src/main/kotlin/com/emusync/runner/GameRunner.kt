package com.emusync.runner

import com.emusync.model.EmulatorSystem
import com.emusync.model.GameEntry
import com.emusync.model.NativePCGame
import com.emusync.steam.SteamShortcutManager
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import java.io.File

/**
 * Result of a game execution.
 *
 * @property exitCode  The process exit code (0 = normal exit, or 0 for external polling).
 * @property durationMs How long the game ran, in milliseconds.
 */
data class GameResult(
    val exitCode: Int,
    val durationMs: Long,
)

/**
 * Universal game launcher that handles both emulator-based and native PC games.
 *
 * Supports two launch modes:
 * - **Direct**: Spawns the process directly via [ProcessBuilder] (default).
 * - **Via Steam**: Uses `steam://rungameid/<appId>` so Steam applies the
 *   configured controller layout. Used when [steamAppId] is provided.
 *
 * Usage:
 * ```
 * val runner = GameRunner()
 *
 * // Direct launch
 * val result = runner.launch(gbaSystem, romFile = File("/roms/pokemon.gba"))
 *
 * // Launch via Steam (applies controller layout)
 * val result = runner.launch(gbaSystem, romFile = romFile, steamAppId = 123456)
 * ```
 */
class GameRunner {

    /**
     * Launches a game and suspends until it finishes.
     *
     * @param entry      The [GameEntry] to launch.
     * @param romFile    Required for [EmulatorSystem]. Ignored for [NativePCGame].
     * @param steamAppId If non-null, launches through Steam to apply controller layout.
     * @return [GameResult] with exit code and duration.
     */
    suspend fun launch(
        entry: GameEntry,
        romFile: File? = null,
        steamAppId: Int? = null,
    ): GameResult = withContext(Dispatchers.IO) {
        val startTime = System.currentTimeMillis()

        if (steamAppId != null) {
            launchViaSteam(entry, romFile, steamAppId)
        } else {
            launchDirect(entry, romFile)
        }

        val durationMs = System.currentTimeMillis() - startTime
        GameResult(exitCode = 0, durationMs = durationMs)
    }

    /**
     * Launches a game directly via [ProcessBuilder] (original behavior).
     */
    private suspend fun launchDirect(entry: GameEntry, romFile: File?) {
        val command = buildCommand(entry, romFile)
        val workingDir = resolveWorkingDirectory(entry, romFile)

        check(command.isNotEmpty()) { "Command list must not be empty" }

        val pb = ProcessBuilder(command)
            .directory(workingDir)
            .inheritIO()

        // Inject scaling fixes and fullscreen flags for emulators
        // (mirrors what the wrapper scripts do for the Steam launch path)
        if (entry is EmulatorSystem) {
            val env = pb.environment()
            env["QT_QPA_PLATFORM"] = "xcb"
            env["QT_ENABLE_HIGHDPI_SCALING"] = "0"
            env["QT_AUTO_SCREEN_SCALE_FACTOR"] = "0"
            env["QT_SCALE_FACTOR"] = "1"
            env["QT_FONT_DPI"] = "96"
            env["GDK_BACKEND"] = "x11"
            env["GDK_SCALE"] = "1"
            env["GDK_DPI_SCALE"] = "1"
            env["XCURSOR_SIZE"] = "24"
        }

        val process = pb.start()

        // If it's a native game with a process-wait, we ignore the initial process exit
        // and instead poll for the named process.
        if (entry is NativePCGame && entry.waitForProcess != null) {
            waitForExternalProcess(entry.waitForProcess)
        } else {
            process.waitFor()
        }
    }

    /**
     * Launches a game through Steam so the configured controller layout is applied.
     *
     * Flow:
     * 1. For emulators: writes the ROM path to a signal file via [SteamShortcutManager]
     * 2. Opens `steam://rungameid/<appId>` to tell Steam to launch the shortcut
     * 3. Polls for the emulator/game process to appear and then exit
     */
    private suspend fun launchViaSteam(entry: GameEntry, romFile: File?, appId: Int) {
        // Step 1: Prepare the launch (write ROM path for emulators)
        if (entry is EmulatorSystem && romFile != null) {
            val manager = SteamShortcutManager()
            manager.prepareLaunch(entry, romFile)
        }

        // Step 2: Tell Steam to launch the shortcut
        // Non-Steam shortcuts use a 64-bit Game ID: (unsigned_appid << 32) | 0x02000000
        val unsignedAppId = appId.toUInt().toLong()
        val gameId = (unsignedAppId shl 32) or 0x02000000L
        ProcessBuilder("steam", "steam://rungameid/$gameId")
            .start()

        // Step 3: Wait for the game/emulator process to appear and exit
        val processName = resolveProcessName(entry)
        if (processName != null) {
            waitForExternalProcess(processName)
        } else {
            // Fallback: wait a reasonable time if we can't detect the process
            delay(5000)
        }
    }

    /**
     * Determines the process name to monitor for a given entry.
     * Used when launching via Steam to detect when the game exits.
     */
    private fun resolveProcessName(entry: GameEntry): String? {
        return when (entry) {
            is EmulatorSystem -> {
                // Use explicit steamProcessName, or derive from executable basename
                entry.steamProcessName ?: File(entry.executablePath).name
            }
            is NativePCGame -> {
                entry.waitForProcess ?: File(entry.executablePath).name
            }
        }
    }

    /**
     * Builds the command list depending on the entry type.
     */
    private fun buildCommand(entry: GameEntry, romFile: File?): List<String> {
        return when (entry) {
            is EmulatorSystem -> {
                requireNotNull(romFile) {
                    "A ROM file must be provided to launch an EmulatorSystem ('${entry.name}')"
                }
                buildList {
                    add(entry.executablePath)
                    // Add fullscreen/Big Picture flags to bypass Qt windowed GUI
                    val fsArgs = SteamShortcutManager.resolveFullscreenArgs(entry)
                    if (fsArgs.isNotBlank()) {
                        fsArgs.split(" ").filter { it.isNotBlank() }.forEach { add(it) }
                    }
                    entry.arguments.forEach { arg ->
                        add(if (arg == "{ROM}") romFile.absolutePath else arg)
                    }
                }
            }
            is NativePCGame -> {
                buildList {
                    add(entry.executablePath)
                    addAll(entry.arguments)
                }
            }
        }
    }

    /**
     * Resolves the working directory for the process.
     */
    private fun resolveWorkingDirectory(entry: GameEntry, romFile: File?): File {
        return when (entry) {
            is EmulatorSystem -> romFile?.parentFile ?: File(".")
            is NativePCGame -> File(entry.executablePath).parentFile ?: File(".")
        }
    }

    /**
     * Polls the system for a process name.
     * 1. Waits up to 30s for the process to appear.
     * 2. Waits indefinitely for the process to disappear.
     */
    private suspend fun waitForExternalProcess(processName: String) = withContext(Dispatchers.IO) {
        // Step 1: Wait for appearance (timeout 30s)
        val appearanceStart = System.currentTimeMillis()
        while (System.currentTimeMillis() - appearanceStart < 30_000) {
            if (isProcessRunning(processName)) break
            kotlinx.coroutines.delay(1000)
        }

        // Step 2: Wait for disappearance
        while (isProcessRunning(processName)) {
            kotlinx.coroutines.delay(2000)
        }
    }

    private fun isProcessRunning(name: String): Boolean {
        return try {
            // Use -f to match against the full command line, since -x can't match
            // process names longer than 15 characters (e.g. AppImage executables).
            val process = ProcessBuilder("pgrep", "-f", name).start()
            process.waitFor() == 0
        } catch (e: Exception) {
            false
        }
    }
}
