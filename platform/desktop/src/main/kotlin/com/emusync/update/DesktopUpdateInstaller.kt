package com.emusync.update

import com.emusync.domain.update.UpdateInstaller
import com.emusync.util.AppImageEnvironment
import java.io.File
import java.util.logging.Level
import java.util.logging.Logger
import kotlin.system.exitProcess

open class DesktopUpdateInstaller(private val exitApplication: (Int) -> Unit = ::exitProcess) :
    UpdateInstaller {
    private val appImageInstaller = AppImageInstaller()
    private val logger = Logger.getLogger(DesktopUpdateInstaller::class.java.name)

    open fun isWindows(): Boolean =
        System.getProperty("os.name", "").lowercase().contains("windows")

    override fun restart(path: String): Boolean = applyUpdateAndRestart(File(path))

    /**
     * Applies the downloaded update and restarts the application. On Windows, launches the
     * installer (.msi or .exe) and terminates EmuSync. On Linux, atomically replaces the AppImage
     * and launches the new executable.
     */
    internal open fun applyUpdateAndRestart(downloadedFile: File): Boolean {
        if (!downloadedFile.exists()) return false

        val applied =
            if (isWindows()) {
                applyWindowsUpdateAndRestart(downloadedFile)
            } else {
                applyLinuxUpdateAndRestart(downloadedFile)
            }
        if (applied) exitApplication(0)
        return applied
    }

    /** Applies an update on Windows by running the installer (.msi or .exe). */
    internal open fun applyWindowsUpdateAndRestart(
        downloadedFile: File,
        launcher: (List<String>) -> Unit = { cmd ->
            ProcessBuilder(cmd).start()
        },
    ): Boolean {
        return try {
            val name = downloadedFile.name.lowercase()
            when {
                name.endsWith(".msi") -> {
                    launcher(listOf("msiexec", "/i", downloadedFile.absolutePath))
                    true
                }
                name.endsWith(".exe") -> {
                    launcher(listOf(downloadedFile.absolutePath))
                    true
                }
                else -> false
            }
        } catch (_: Throwable) {
            false
        }
    }

    /**
     * Resolves the path to the currently running AppImage on Linux. Defaults to the APPIMAGE
     * environment variable set by the AppImage runtime.
     */
    open fun getCurrentAppImagePath(): String? =
        System.getenv("APPIMAGE")?.takeIf { it.isNotBlank() }

    /**
     * Indicates whether the application is running in Steam Deck Gaming Mode (gamescope session).
     * Can be overridden in tests to simulate SteamOS Game Mode.
     */
    override fun isSteamGameMode(): Boolean =
        System.getenv("SteamGamepadUI") == "1" ||
            System.getenv("XDG_CURRENT_DESKTOP")?.lowercase() == "gamescope"

    /**
     * Starts the updated AppImage with independent standard streams. In Steam Deck Game Mode, Steam
     * manages the process tree; exiting cleanly lets the user relaunch the updated app from Steam
     * without leaving ghost processes. Application exit is handled after installation by
     * [applyUpdateAndRestart].
     */
    internal open fun defaultLinuxLauncher(path: String) {
        if (isSteamGameMode()) return

        val builder =
            ProcessBuilder(path)
                .redirectInput(File("/dev/null"))
                .redirectOutput(ProcessBuilder.Redirect.DISCARD)
                .redirectError(ProcessBuilder.Redirect.DISCARD)
        AppImageEnvironment.clean(builder.environment())
        builder.start()
    }

    /** Replaces the currently running AppImage on Linux and restarts the application. */
    // This adapter boundary reports any filesystem or launcher exception after installer rollback.
    @Suppress("TooGenericExceptionCaught")
    internal open fun applyLinuxUpdateAndRestart(
        downloadedFile: File,
        launcher: (String) -> Unit = ::defaultLinuxLauncher,
    ): Boolean {
        val currentAppImagePath = getCurrentAppImagePath() ?: return false
        val currentAppImage = File(currentAppImagePath)
        return if (!currentAppImage.exists() || !downloadedFile.exists()) false
        else
            try {
                appImageInstaller.replace(downloadedFile, currentAppImage) {
                    launcher(currentAppImage.absolutePath)
                }
                true
            } catch (failure: Exception) {
                logger.log(
                    Level.WARNING,
                    "Could not install or launch the updated AppImage at $currentAppImagePath",
                    failure,
                )
                false
            }
    }
}
