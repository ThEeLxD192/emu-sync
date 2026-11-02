package com.emusync.runner

import com.emusync.domain.execution.ArgumentsCodec
import com.emusync.domain.execution.GameLaunchRequest
import com.emusync.domain.execution.GameLauncher
import com.emusync.domain.execution.GameResult
import com.emusync.domain.execution.SteamLaunchPreparation
import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import com.emusync.steam.SteamShortcutManager
import com.emusync.util.AppImageEnvironment
import java.io.File
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext

/**
 * Universal game launcher that handles both emulator-based and native PC games.
 *
 * Supports two launch modes:
 * - **Direct**: Spawns the process directly via [ProcessBuilder] (default).
 * - **Via Steam**: Uses `steam://rungameid/<appId>` so Steam applies the configured controller
 *   layout. Used when the launch request specifies a Steam app ID.
 *
 * Usage:
 * ```
 * val runner = GameRunner(steamLaunches)
 *
 * // Direct launch
 * val result = runner.launch(GameLaunchRequest(gbaSystem, romPath = "/roms/pokemon.gba"))
 *
 * // Launch via Steam (applies controller layout)
 * val result = runner.launch(GameLaunchRequest(gbaSystem, romPath = romFile.path, steamAppId = 123456))
 * ```
 */
class GameRunner(
    private val steamLaunches: SteamLaunchPreparation,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
) : GameLauncher {
    private val processes = ProcessMonitor()
    private val tracker =
        ExternalProcessTracker(processes::matchingProcessIds, processes::isSteamShaderCompiling)

    companion object {
        private const val STEAM_HANDOFF_DELAY_MS = 5_000L
    }

    /** Opens an emulator's own interface without supplying a ROM or waiting for it to exit. */
    suspend fun openEmulator(entry: EmulatorSystem): Process =
        withContext(ioDispatcher) {
            val command = buildList {
                add(entry.executablePath)
                addAll(entry.arguments.filterNot { it.contains("{ROM}") })
            }
            val workingDir = File(entry.executablePath).parentFile?.takeIf { it.exists() }
            val processBuilder = ProcessBuilder(command)
            if (workingDir != null) processBuilder.directory(workingDir)
            AppImageEnvironment.clean(processBuilder.environment())
            processBuilder
                .redirectOutput(ProcessBuilder.Redirect.DISCARD)
                .redirectError(ProcessBuilder.Redirect.DISCARD)
                .start()
        }

    /**
     * Launches a game and suspends until it finishes.
     *
     * @param request Entry, optional ROM path and optional Steam app ID for the session.
     * @return [GameResult] with exit code and duration.
     */
    override suspend fun launch(request: GameLaunchRequest): GameResult =
        withContext(ioDispatcher) {
            val startTime = System.currentTimeMillis()
            val entry = request.entry
            val romFile = request.romPath?.let(::File)
            val steamAppId = request.steamAppId

            val success =
                if (steamAppId != null) {
                    launchViaSteam(entry, romFile, steamAppId)
                } else {
                    launchDirect(entry, romFile)
                }

            val durationMs = System.currentTimeMillis() - startTime
            GameResult(exitCode = if (success) 0 else 1, durationMs = durationMs)
        }

    /** Launches a game directly via [ProcessBuilder] (original behavior). */
    private suspend fun launchDirect(entry: GameEntry, romFile: File?): Boolean {
        val command = buildCommand(entry, romFile)
        val workingDir = resolveWorkingDirectory(entry, romFile)
        val waitForProcess = (entry as? NativePCGame)?.waitForProcess
        val existingMatches = waitForProcess?.let(::matchingProcessIds).orEmpty()

        check(command.isNotEmpty()) { "Command list must not be empty" }

        val pb = ProcessBuilder(command).directory(workingDir).inheritIO()

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
            steamLaunches.prepare(entry, romFile.absolutePath)
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
            delay(STEAM_HANDOFF_DELAY_MS)
            true
        }
    }

    /**
     * Determines the process name to monitor for a given entry. Used when launching via Steam to
     * detect when the game exits.
     */
    private fun resolveProcessName(entry: GameEntry): String? {
        return when (entry) {
            is EmulatorSystem -> {
                // Use explicit steamProcessName, or derive from executable basename
                entry.steamProcessName
                    ?: entry.executablePath.replace('\\', '/').substringAfterLast('/').ifBlank {
                        null
                    }
            }
            is NativePCGame -> {
                entry.waitForProcess
                    ?: entry.executablePath.replace('\\', '/').substringAfterLast('/').ifBlank {
                        null
                    }
            }
        }
    }

    /** Builds the command list depending on the entry type. */
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
                        addAll(
                            requireNotNull(ArgumentsCodec.parse(fsArgs)) {
                                "Fullscreen arguments contain unmatched quotes"
                            }
                        )
                    }
                    entry.arguments.forEach { arg ->
                        add(arg.replace("{ROM}", romFile.absolutePath))
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

    /** Resolves the working directory for the process. */
    private fun resolveWorkingDirectory(entry: GameEntry, romFile: File?): File {
        return when (entry) {
            is EmulatorSystem -> romFile?.parentFile ?: File(".")
            is NativePCGame -> File(entry.executablePath).parentFile ?: File(".")
        }
    }

    /**
     * Polls the system for a process name.
     * 1. Waits up to 60s for the process to appear. If Steam is actively compiling Vulkan shaders
     *    in the background (via fossilize_replay), the timeout is continuously renewed so long
     *    shader compilation steps (even 5+ mins) are not prematurely terminated.
     * 2. Waits indefinitely for the process to disappear once it has appeared.
     *
     * @return true if the process appeared and then finished, false if it never appeared within the
     *   timeout.
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
    ): Boolean =
        withContext(ioDispatcher) {
            tracker.waitForProcess(
                ProcessLaunch(
                    processName,
                    existingProcessIds,
                    launchRequest,
                    launchHandoffSucceeded,
                ),
                ProcessPolling(
                    appearanceTimeoutMs,
                    pollIntervalMs,
                    exitPollIntervalMs,
                    reuseConfirmationDelayMs,
                ),
            )
        }

    internal fun isSteamShaderCompiling(): Boolean = processes.isSteamShaderCompiling()

    internal fun isProcessRunning(name: String): Boolean = processes.isProcessRunning(name)

    internal fun matchingProcessIds(name: String): Set<Long> = processes.matchingProcessIds(name)
}

private data class LaunchRequest(
    val process: Process? = null,
    val desktopHandoffSucceeded: Boolean = false,
)

/** Launches a steam:// URI in a cross-platform manner. */
private fun launchSteamUri(uri: String): LaunchRequest? {
    val os = System.getProperty("os.name", "").lowercase()
    try {
        val process =
            if (os.contains("win")) {
                ProcessBuilder("cmd", "/c", "start", uri).start()
            } else {
                ProcessBuilder("steam", uri).start()
            }
        return LaunchRequest(process = process)
    } catch (_: Exception) {
        return try {
            if (
                java.awt.Desktop.isDesktopSupported() &&
                    java.awt.Desktop.getDesktop().isSupported(java.awt.Desktop.Action.BROWSE)
            ) {
                java.awt.Desktop.getDesktop().browse(java.net.URI(uri))
                LaunchRequest(desktopHandoffSucceeded = true)
            } else null
        } catch (_: Exception) {
            null
        }
    }
}
