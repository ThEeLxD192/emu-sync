package com.emusync.update

import com.emusync.AppInfo
import com.emusync.util.AppImageEnvironment
import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.engine.cio.CIO
import io.ktor.client.plugins.HttpTimeout
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.client.request.get
import io.ktor.client.request.header
import io.ktor.client.statement.HttpResponse
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.serialization.kotlinx.json.json
import java.io.File
import java.util.logging.Level
import java.util.logging.Logger
import kotlin.system.exitProcess
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

@Serializable
data class GitHubAsset(
    val name: String,
    @SerialName("browser_download_url") val downloadUrl: String,
    val size: Long = 0L,
)

@Serializable
data class GitHubRelease(
    @SerialName("tag_name") val tagName: String,
    val name: String? = null,
    val body: String? = null,
    val prerelease: Boolean = false,
    val draft: Boolean = false,
    val assets: List<GitHubAsset> = emptyList(),
)

data class UpdateInfo(
    val version: String,
    val tagName: String,
    val releaseNotes: String,
    val downloadUrl: String,
    val assetSize: Long,
    val assetName: String = "",
)

open class UpdateManager(
    private val client: HttpClient = createUpdateClient(),
    private val exitApplication: (Int) -> Unit = ::exitProcess,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
) : AutoCloseable {
    override fun close() = client.close()

    private val appImageInstaller = AppImageInstaller()
    private val downloader = UpdateDownloader(client, ioDispatcher)
    private val logger = Logger.getLogger(UpdateManager::class.java.name)

    /**
     * Indicates whether the host OS is Windows. Can be overridden in tests to simulate Windows
     * environment.
     */
    open fun isWindows(): Boolean =
        System.getProperty("os.name", "").lowercase().contains("windows")

    /** Finds the best asset for the current operating system. */
    open fun findBestAsset(assets: List<GitHubAsset>): GitHubAsset? {
        return findBestAssetForPlatform(assets, isWindows = isWindows())
    }

    /** Checks GitHub for the latest release and compares with the running version. */
    suspend fun checkForUpdates(
        owner: String = AppInfo.GITHUB_OWNER,
        repo: String = AppInfo.GITHUB_REPO,
        currentVersion: String = AppInfo.VERSION,
    ): UpdateInfo? =
        withContext(ioDispatcher) {
            try {
                val response: HttpResponse =
                    client.get("https://api.github.com/repos/$owner/$repo/releases/latest") {
                        header(HttpHeaders.Accept, "application/vnd.github+json")
                        header(HttpHeaders.UserAgent, "EmuSync-Desktop-App")
                    }

                if (response.status != HttpStatusCode.OK) {
                    return@withContext null
                }

                val release: GitHubRelease = response.body()
                val remoteVersion = release.tagName.removePrefix("v").removePrefix("V").trim()

                if (!isNewerVersion(current = currentVersion, remote = remoteVersion)) {
                    return@withContext null
                }

                // Find platform-appropriate asset (.msi/.exe on Windows, .AppImage on Linux)
                val asset = findBestAsset(release.assets) ?: return@withContext null

                UpdateInfo(
                    version = remoteVersion,
                    tagName = release.tagName,
                    releaseNotes =
                        release.body ?: release.name ?: "New version $remoteVersion available.",
                    downloadUrl = asset.downloadUrl,
                    assetSize = asset.size,
                    assetName = asset.name,
                )
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (_: Exception) {
                null
            }
        }

    /** Downloads the asset from [downloadUrl] to [destinationFile] reporting progress. */
    suspend fun downloadUpdate(
        downloadUrl: String,
        destinationFile: File,
        onProgress: (Float) -> Unit,
    ): Result<Unit> = downloader.download(downloadUrl, destinationFile, onProgress)

    /**
     * Applies the downloaded update and restarts the application. On Windows, launches the
     * installer (.msi or .exe) and terminates EmuSync. On Linux, atomically replaces the AppImage
     * and launches the new executable.
     */
    open fun applyUpdateAndRestart(downloadedFile: File): Boolean {
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
    open fun applyWindowsUpdateAndRestart(
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
    open fun isSteamGameMode(): Boolean =
        System.getenv("SteamGamepadUI") == "1" ||
            System.getenv("XDG_CURRENT_DESKTOP")?.lowercase() == "gamescope"

    /**
     * Starts the updated AppImage with independent standard streams. In Steam Deck Game Mode, Steam
     * manages the process tree; exiting cleanly lets the user relaunch the updated app from Steam
     * without leaving ghost processes. Application exit is handled after installation by
     * [applyUpdateAndRestart].
     */
    open fun defaultLinuxLauncher(path: String) {
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
    // This UI boundary reports any filesystem or launcher exception after installer rollback.
    @Suppress("TooGenericExceptionCaught")
    open fun applyLinuxUpdateAndRestart(
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

    companion object {
        /**
         * Selects the most appropriate asset from a release based on platform priority. Windows:
         * .msi > .exe > .zip Linux: .AppImage > .tar.gz > .deb
         */
        fun findBestAssetForPlatform(assets: List<GitHubAsset>, isWindows: Boolean): GitHubAsset? {
            return if (isWindows) {
                assets.firstOrNull { it.name.endsWith(".msi", ignoreCase = true) }
                    ?: assets.firstOrNull { it.name.endsWith(".exe", ignoreCase = true) }
                    ?: assets.firstOrNull { it.name.endsWith(".zip", ignoreCase = true) }
            } else {
                assets.firstOrNull { it.name.endsWith(".AppImage", ignoreCase = true) }
                    ?: assets.firstOrNull { it.name.endsWith(".tar.gz", ignoreCase = true) }
                    ?: assets.firstOrNull { it.name.endsWith(".deb", ignoreCase = true) }
            }
        }

        /** Compares semantic versions (e.g. 0.1.0 vs 0.1.1 or 0.2.0 vs 0.1.9). */
        fun isNewerVersion(current: String, remote: String): Boolean {
            val currParts = current.split('.').mapNotNull { it.trim().toIntOrNull() }
            val remoteParts = remote.split('.').mapNotNull { it.trim().toIntOrNull() }

            val maxLen = maxOf(currParts.size, remoteParts.size)
            for (i in 0 until maxLen) {
                val c = currParts.getOrElse(i) { 0 }
                val r = remoteParts.getOrElse(i) { 0 }
                val comparison = r.compareTo(c)
                if (comparison != 0) return comparison > 0
            }
            return false
        }
    }
}

private const val UPDATE_CONNECT_TIMEOUT_MS = 15_000L

private fun createUpdateClient(): HttpClient =
    HttpClient(CIO) {
        install(ContentNegotiation) {
            json(
                Json {
                    ignoreUnknownKeys = true
                    isLenient = true
                }
            )
        }
        install(HttpTimeout) {
            connectTimeoutMillis = UPDATE_CONNECT_TIMEOUT_MS
            socketTimeoutMillis = UPDATE_SOCKET_TIMEOUT_MS
            requestTimeoutMillis = null
        }
    }
