package com.emusync.runner

import com.emusync.model.EmulatorSystem
import com.emusync.model.GameEntry
import com.emusync.model.NativePCGame
import com.emusync.steam.SteamShortcutManager
import com.emusync.util.AppImageEnvironment
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

    private data class LaunchRequest(
        val process: Process? = null,
        val desktopHandoffSucceeded: Boolean = false,
    )

    /** Opens an emulator's own interface without supplying a ROM or waiting for it to exit. */
    suspend fun openEmulator(entry: EmulatorSystem) = withContext(Dispatchers.IO) {
        val command = buildList {
            add(entry.executablePath)
            addAll(entry.arguments.filterNot { it.contains("{ROM}") })
        }
        val workingDir = File(entry.executablePath).parentFile?.takeIf { it.exists() }
        val processBuilder = ProcessBuilder(command)
        if (workingDir != null) processBuilder.directory(workingDir)
        AppImageEnvironment.clean(processBuilder.environment())
        processBuilder.redirectOutput(ProcessBuilder.Redirect.DISCARD)
            .redirectError(ProcessBuilder.Redirect.DISCARD)
            .start()
    }

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

        val success = if (steamAppId != null) {
            launchViaSteam(entry, romFile, steamAppId)
        } else {
            launchDirect(entry, romFile)
        }

        val durationMs = System.currentTimeMillis() - startTime
        GameResult(exitCode = if (success) 0 else 1, durationMs = durationMs)
    }

    /**
     * Launches a game directly via [ProcessBuilder] (original behavior).
     */
    private suspend fun launchDirect(entry: GameEntry, romFile: File?): Boolean {
        val command = buildCommand(entry, romFile)
        val workingDir = resolveWorkingDirectory(entry, romFile)
        val waitForProcess = (entry as? NativePCGame)?.waitForProcess
        val existingMatches = waitForProcess?.let(::matchingProcessIds).orEmpty()

        check(command.isNotEmpty()) { "Command list must not be empty" }

        val pb = ProcessBuilder(command)
            .directory(workingDir)
            .inheritIO()

        if (entry is EmulatorSystem) {
            AppImageEnvironment.clean(pb.environment())
        }

        val process = pb.start()

        // If it's a native game with a process-wait, we ignore the initial process exit
        // and instead poll for the named process.
        return if (waitForProcess != null) {
            waitForExternalProcess(waitForProcess, existingMatches, launchRequest = process)
        } else {
            process.waitFor() == 0
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
    private suspend fun launchViaSteam(entry: GameEntry, romFile: File?, appId: Int): Boolean {
        // Step 1: Prepare the launch (write ROM path for emulators)
        if (entry is EmulatorSystem && romFile != null) {
            val manager = SteamShortcutManager()
            manager.prepareLaunch(entry, romFile)
        }

        // Step 2: Tell Steam to launch the shortcut
        // Non-Steam shortcuts use a 64-bit Game ID: (unsigned_appid << 32) | 0x02000000
        val unsignedAppId = appId.toUInt().toLong()
        val gameId = (unsignedAppId shl 32) or 0x02000000L
        val processName = resolveProcessName(entry)
        val existingMatches = processName?.let(::matchingProcessIds).orEmpty()
        val launchRequest = launchSteamUri("steam://rungameid/$gameId")

        // Step 3: Wait for a newly-created game/emulator process to appear and exit
        return if (processName != null) {
            waitForExternalProcess(
                processName,
                existingMatches,
                launchRequest = launchRequest?.process,
                launchHandoffSucceeded = launchRequest?.desktopHandoffSucceeded == true,
            )
        } else {
            // Fallback: wait a reasonable time if we can't detect the process
            delay(5000)
            true
        }
    }

    /**
     * Launches a steam:// URI in a cross-platform manner.
     */
    private fun launchSteamUri(uri: String): LaunchRequest? {
        val os = System.getProperty("os.name", "").lowercase()
        try {
            val process = if (os.contains("win")) {
                ProcessBuilder("cmd", "/c", "start", uri).start()
            } else {
                ProcessBuilder("steam", uri).start()
            }
            return LaunchRequest(process = process)
        } catch (_: Exception) {
            try {
                if (java.awt.Desktop.isDesktopSupported() && java.awt.Desktop.getDesktop().isSupported(java.awt.Desktop.Action.BROWSE)) {
                    java.awt.Desktop.getDesktop().browse(java.net.URI(uri))
                    return LaunchRequest(desktopHandoffSucceeded = true)
                }
            } catch (_: Exception) {
                // Ignore fallback failure
            }
        }
        return null
    }

    /**
     * Determines the process name to monitor for a given entry.
     * Used when launching via Steam to detect when the game exits.
     */
    private fun resolveProcessName(entry: GameEntry): String? {
        return when (entry) {
            is EmulatorSystem -> {
                // Use explicit steamProcessName, or derive from executable basename
                entry.steamProcessName ?: entry.executablePath.replace('\\', '/').substringAfterLast('/').ifBlank { null }
            }
            is NativePCGame -> {
                entry.waitForProcess ?: entry.executablePath.replace('\\', '/').substringAfterLast('/').ifBlank { null }
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
     * 1. Waits up to 60s for the process to appear.
     *    If Steam is actively compiling Vulkan shaders in the background (via fossilize_replay),
     *    the timeout is continuously renewed so long shader compilation steps (even 5+ mins)
     *    are not prematurely terminated.
     * 2. Waits indefinitely for the process to disappear once it has appeared.
     *
     * @return true if the process appeared and then finished, false if it never appeared within the timeout.
     */
    internal suspend fun waitForExternalProcess(
        processName: String,
        existingProcessIds: Set<Long> = emptySet(),
        launchRequest: Process? = null,
        launchHandoffSucceeded: Boolean = false,
        appearanceTimeoutMs: Long = 60_000L,
        pollIntervalMs: Long = 1_000L,
        exitPollIntervalMs: Long = 2_000L,
        reuseConfirmationDelayMs: Long = 1_000L,
    ): Boolean = withContext(Dispatchers.IO) {
        // Step 1: Wait for appearance (base timeout 60s, dynamically extended while Steam shaders compile)
        val trackedProcessIds = mutableSetOf<Long>()
        var appeared = false
        var appearanceStart = System.currentTimeMillis()
        var successfulLaunchRequestAt: Long? = null

        while (System.currentTimeMillis() - appearanceStart < appearanceTimeoutMs) {
            val newMatches = matchingProcessIds(processName) - existingProcessIds
            if (newMatches.isNotEmpty()) {
                trackedProcessIds += newMatches
                appeared = true
                break
            }

            val launchExitCode = if (launchRequest?.isAlive == false) {
                runCatching { launchRequest.exitValue() }.getOrNull()
            } else {
                null
            }
            if (launchRequest?.isAlive == false) {
                when {
                    launchExitCode == null -> Unit
                    launchExitCode != 0 -> break
                }
            }

            if (launchHandoffSucceeded || launchExitCode == 0) {
                val now = System.currentTimeMillis()
                val confirmedAt = successfulLaunchRequestAt ?: now.also { successfulLaunchRequestAt = it }
                if (now - confirmedAt >= reuseConfirmationDelayMs) {
                    val reusedMatches = matchingProcessIds(processName).intersect(existingProcessIds)
                    if (reusedMatches.isNotEmpty()) {
                        trackedProcessIds += reusedMatches
                        appeared = true
                        break
                    }
                }
            } else {
                successfulLaunchRequestAt = null
            }

            // If Steam is compiling Vulkan shaders in the background, reset timeout window
            if (isSteamShaderCompiling()) {
                appearanceStart = System.currentTimeMillis()
            }

            kotlinx.coroutines.delay(pollIntervalMs)
        }

        // Step 2: Wait for disappearance (indefinite, only if the process appeared)
        if (trackedProcessIds.isNotEmpty()) {
            while (true) {
                val activeMatches = matchingProcessIds(processName).filterTo(mutableSetOf()) {
                    it !in existingProcessIds || it in trackedProcessIds
                }
                trackedProcessIds += activeMatches
                trackedProcessIds.removeIf { pid -> ProcessHandle.of(pid).map { it.isAlive }.orElse(false).not() }
                if (trackedProcessIds.isEmpty()) break
                kotlinx.coroutines.delay(exitPollIntervalMs)
            }
        }
        appeared
    }

    /**
     * Checks if Steam's Vulkan shader pre-compilation tool (fossilize_replay) is currently active.
     */
    internal fun isSteamShaderCompiling(): Boolean {
        return isProcessRunning("fossilize_replay") || isProcessRunning("fossilize_replay64")
    }

    /**
     * Checks if a process with [name] is currently running on the system.
     *
     * Uses Java 9+ [ProcessHandle] as the primary cross-platform mechanism (Windows, Linux, macOS).
     * Falls back to platform-specific CLI tools (tasklist on Windows, pgrep on Linux/macOS) if needed.
     */
    internal fun isProcessRunning(name: String): Boolean {
        val cleanName = name.trim().removeSurrounding("\"")
        if (cleanName.isBlank()) return false
        try {
            return matchingProcessIds(cleanName).isNotEmpty()
        } catch (_: Exception) {
            // Proceed to fallback
        }

        // 2. OS-specific fallback CLI tools
        return fallbackProcessCheck(cleanName)
    }

    internal fun matchingProcessIds(name: String): Set<Long> {
        val cleanName = File(name.trim().removeSurrounding("\"")).name
        if (cleanName.isBlank()) return emptySet()
        val targetWithoutExtension = File(cleanName).nameWithoutExtension

        return ProcessHandle.allProcesses().filter { it.isAlive && processMatches(it, cleanName, targetWithoutExtension) }
            .map { it.pid() }
            .toList()
            .toSet()
    }

    private fun processMatches(handle: ProcessHandle, targetName: String, targetWithoutExtension: String): Boolean {
        val info = handle.info()
        val commandName = info.command().orElse(null)?.let { File(it).name }
        if (commandName != null && (
                commandName.equals(targetName, ignoreCase = true) ||
                File(commandName).nameWithoutExtension.equals(targetWithoutExtension, ignoreCase = true)
            )) return true

        return info.arguments().orElse(emptyArray()).any { argument ->
            val argumentName = File(argument.trim().removeSurrounding("\"")).name
            argumentName.equals(targetName, ignoreCase = true) ||
                File(argumentName).nameWithoutExtension.equals(targetWithoutExtension, ignoreCase = true)
        }
    }

    private fun fallbackProcessCheck(cleanName: String): Boolean {
        val os = System.getProperty("os.name", "").lowercase()
        return try {
            if (os.contains("win")) {
                val targetExe = if (cleanName.endsWith(".exe", ignoreCase = true)) cleanName else "$cleanName.exe"
                val pb = ProcessBuilder("tasklist", "/FI", "IMAGENAME eq $targetExe", "/NH")
                val process = pb.start()
                val output = process.inputStream.bufferedReader().readText()
                process.waitFor()
                output.contains(targetExe, ignoreCase = true)
            } else {
                val process = ProcessBuilder("pgrep", "-x", File(cleanName).nameWithoutExtension).start()
                process.waitFor() == 0
            }
        } catch (_: Exception) {
            false
        }
    }
}
