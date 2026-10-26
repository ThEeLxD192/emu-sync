package com.emusync.update

import com.emusync.util.AppImageEnvironment

import com.emusync.AppInfo
import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.engine.cio.CIO
import io.ktor.client.plugins.HttpTimeout
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.client.plugins.timeout
import io.ktor.client.request.get
import io.ktor.client.request.header
import io.ktor.client.statement.HttpResponse
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.contentLength
import io.ktor.serialization.kotlinx.json.json
import io.ktor.utils.io.ByteReadChannel
import io.ktor.utils.io.readAvailable
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.io.File
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.AtomicMoveNotSupportedException
import java.nio.file.StandardCopyOption
import java.util.logging.Level
import java.util.logging.Logger
import kotlin.system.exitProcess

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
    private val client: HttpClient = HttpClient(CIO) {
        install(ContentNegotiation) {
            json(Json {
                ignoreUnknownKeys = true
                isLenient = true
            })
        }
        install(HttpTimeout) {
            connectTimeoutMillis = 15_000
            socketTimeoutMillis = 60_000
            requestTimeoutMillis = null
        }
    },
    private val exitApplication: (Int) -> Unit = ::exitProcess,
) : AutoCloseable {
    override fun close() = client.close()

    private val appImageInstaller = AppImageInstaller()
    private val logger = Logger.getLogger(UpdateManager::class.java.name)

    /**
     * Indicates whether the host OS is Windows.
     * Can be overridden in tests to simulate Windows environment.
     */
    open fun isWindows(): Boolean =
        System.getProperty("os.name", "").lowercase().contains("windows")

    /**
     * Finds the best asset for the current operating system.
     */
    open fun findBestAsset(assets: List<GitHubAsset>): GitHubAsset? {
        return findBestAssetForPlatform(assets, isWindows = isWindows())
    }

    /**
     * Checks GitHub for the latest release and compares with the running version.
     */
    suspend fun checkForUpdates(
        owner: String = AppInfo.GITHUB_OWNER,
        repo: String = AppInfo.GITHUB_REPO,
        currentVersion: String = AppInfo.VERSION,
    ): UpdateInfo? = withContext(Dispatchers.IO) {
        try {
            val response: HttpResponse = client.get("https://api.github.com/repos/$owner/$repo/releases/latest") {
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
                releaseNotes = release.body ?: release.name ?: "New version $remoteVersion available.",
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

    /**
     * Downloads the asset from [downloadUrl] to [destinationFile] reporting progress.
     */
    suspend fun downloadUpdate(
        downloadUrl: String,
        destinationFile: File,
        onProgress: (Float) -> Unit,
    ): Result<Unit> = withContext(Dispatchers.IO) {
        var staged: Path? = null
        var channel: ByteReadChannel? = null
        try {
            val parent = destinationFile.absoluteFile.parentFile
            Files.createDirectories(parent.toPath())
            staged = Files.createTempFile(parent.toPath(), ".${destinationFile.name}.", ".part")
            val response: HttpResponse = client.get(downloadUrl) {
                header(HttpHeaders.UserAgent, "EmuSync-Desktop-App")
                timeout {
                    requestTimeoutMillis = null
                    socketTimeoutMillis = 60_000
                }
            }
            if (response.status != HttpStatusCode.OK) {
                return@withContext Result.failure(
                    IllegalStateException("HTTP ${response.status.value} ${response.status.description}"))
            }
            val totalBytes = response.contentLength()
            val body: ByteReadChannel = response.body()
            channel = body
            var bytesRead = 0L
            Files.newOutputStream(staged).use { output ->
                val buffer = ByteArray(64 * 1024)
                while (!body.isClosedForRead) {
                    currentCoroutineContext().ensureActive()
                    val read = body.readAvailable(buffer, 0, buffer.size)
                    if (read < 0) break
                    if (read == 0) continue
                    output.write(buffer, 0, read)
                    bytesRead += read
                    if (totalBytes != null && totalBytes > 0) {
                        onProgress((bytesRead.toFloat() / totalBytes).coerceIn(0f, 1f))
                    }
                }
            }
            check(bytesRead > 0) { "The downloaded update is empty" }
            check(totalBytes == null || bytesRead == totalBytes) { "The downloaded update is incomplete: expected $totalBytes bytes, received $bytesRead" }
            currentCoroutineContext().ensureActive()
            staged.toFile().setExecutable(true, false)
            try {
                Files.move(staged, destinationFile.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
            } catch (_: AtomicMoveNotSupportedException) {
                Files.move(staged, destinationFile.toPath(), StandardCopyOption.REPLACE_EXISTING)
            }
            staged = null
            onProgress(1f)
            Result.success(Unit)
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (failure: Exception) {
            Result.failure(failure)
        } finally {
            channel?.cancel(CancellationException("Update response is no longer needed"))
            staged?.let { Files.deleteIfExists(it) }
        }
    }

    /**
     * Applies the downloaded update and restarts the application.
     * On Windows, launches the installer (.msi or .exe) and terminates EmuSync.
     * On Linux, atomically replaces the AppImage and launches the new executable.
     */
    open fun applyUpdateAndRestart(downloadedFile: File): Boolean {
        if (!downloadedFile.exists()) return false

        val applied = if (isWindows()) {
            applyWindowsUpdateAndRestart(downloadedFile)
        } else {
            applyLinuxUpdateAndRestart(downloadedFile)
        }
        if (applied) exitApplication(0)
        return applied
    }

    /**
     * Applies an update on Windows by running the installer (.msi or .exe).
     */
    open fun applyWindowsUpdateAndRestart(
        downloadedFile: File,
        launcher: (List<String>) -> Unit = { cmd ->
            ProcessBuilder(cmd).start()
        }
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
     * Resolves the path to the currently running AppImage on Linux.
     * Defaults to the APPIMAGE environment variable set by the AppImage runtime.
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
     * Replaces the running [target] AppImage with [source].
     * Stages the download on the destination filesystem before replacing the active binary.
     */
    internal fun replaceAppImageFile(source: File, target: File) {
        appImageInstaller.replace(source, target)
    }

    /**
     * Starts the updated AppImage with independent standard streams.
     * In Steam Deck Game Mode, Steam manages the process tree; exiting cleanly lets
     * the user relaunch the updated app from Steam without leaving ghost processes.
     * Application exit is handled after installation by [applyUpdateAndRestart].
     */
    open fun defaultLinuxLauncher(path: String) {
        if (isSteamGameMode()) return

        val builder = ProcessBuilder(path)
            .redirectInput(File("/dev/null"))
            .redirectOutput(ProcessBuilder.Redirect.DISCARD)
            .redirectError(ProcessBuilder.Redirect.DISCARD)
        AppImageEnvironment.clean(builder.environment())
        builder.start()
    }

    /**
     * Replaces the currently running AppImage on Linux and restarts the application.
     */
    open fun applyLinuxUpdateAndRestart(
        downloadedFile: File,
        launcher: (String) -> Unit = ::defaultLinuxLauncher
    ): Boolean {
        val currentAppImagePath = getCurrentAppImagePath() ?: return false
        val currentAppImage = File(currentAppImagePath)
        if (!currentAppImage.exists() || !downloadedFile.exists()) return false

        return try {
            appImageInstaller.replace(downloadedFile, currentAppImage) {
                launcher(currentAppImage.absolutePath)
            }
            true
        } catch (failure: Exception) {
            logger.log(Level.WARNING, "Could not install or launch the updated AppImage at $currentAppImagePath", failure)
            false
        }
    }

    companion object {
        /**
         * Selects the most appropriate asset from a release based on platform priority.
         * Windows: .msi > .exe > .zip
         * Linux: .AppImage > .tar.gz > .deb
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

        /**
         * Compares semantic versions (e.g. 0.1.0 vs 0.1.1 or 0.2.0 vs 0.1.9).
         */
        fun isNewerVersion(current: String, remote: String): Boolean {
            val currParts = current.split('.').mapNotNull { it.trim().toIntOrNull() }
            val remoteParts = remote.split('.').mapNotNull { it.trim().toIntOrNull() }

            val maxLen = maxOf(currParts.size, remoteParts.size)
            for (i in 0 until maxLen) {
                val c = currParts.getOrElse(i) { 0 }
                val r = remoteParts.getOrElse(i) { 0 }
                if (r > c) return true
                if (r < c) return false
            }
            return false
        }
    }
}
