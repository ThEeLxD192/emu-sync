package com.emusync.update

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
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.io.File
import java.nio.file.Files
import java.nio.file.StandardCopyOption
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
    }
) {
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
        } catch (_: Throwable) {
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
        try {
            destinationFile.parentFile?.mkdirs()
            if (destinationFile.exists()) {
                destinationFile.delete()
            }

            val response: HttpResponse = client.get(downloadUrl) {
                header(HttpHeaders.UserAgent, "EmuSync-Desktop-App")
                timeout {
                    requestTimeoutMillis = null
                    socketTimeoutMillis = 60_000
                }
            }

            if (response.status != HttpStatusCode.OK) {
                return@withContext Result.failure(
                    IllegalStateException("HTTP ${response.status.value} ${response.status.description}")
                )
            }

            val totalBytes = response.contentLength() ?: -1L
            val channel: ByteReadChannel = response.body()
            var bytesRead = 0L

            destinationFile.outputStream().use { output ->
                val buffer = ByteArray(64 * 1024)
                while (!channel.isClosedForRead) {
                    val read = channel.readAvailable(buffer, 0, buffer.size)
                    if (read <= 0) break
                    output.write(buffer, 0, read)
                    bytesRead += read

                    if (totalBytes > 0) {
                        val progress = (bytesRead.toFloat() / totalBytes).coerceIn(0f, 1f)
                        onProgress(progress)
                    }
                }
            }
            onProgress(1f)
            destinationFile.setExecutable(true, false)
            Result.success(Unit)
        } catch (e: Throwable) {
            destinationFile.delete()
            Result.failure(e)
        }
    }

    /**
     * Applies the downloaded update and restarts the application.
     * On Windows, launches the installer (.msi or .exe) and terminates EmuSync.
     * On Linux, atomically replaces the AppImage and launches the new executable.
     */
    open fun applyUpdateAndRestart(downloadedFile: File): Boolean {
        if (!downloadedFile.exists()) return false

        return if (isWindows()) {
            applyWindowsUpdateAndRestart(downloadedFile)
        } else {
            applyLinuxUpdateAndRestart(downloadedFile)
        }
    }

    /**
     * Applies an update on Windows by running the installer (.msi or .exe).
     */
    open fun applyWindowsUpdateAndRestart(
        downloadedFile: File,
        launcher: (List<String>) -> Unit = { cmd ->
            ProcessBuilder(cmd).start()
            exitProcess(0)
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
     * Replaces the currently running AppImage on Linux and restarts the application.
     */
    open fun applyLinuxUpdateAndRestart(
        downloadedFile: File,
        launcher: (String) -> Unit = { path ->
            ProcessBuilder(path).start()
            exitProcess(0)
        }
    ): Boolean {
        val currentAppImagePath = System.getenv("APPIMAGE") ?: return false
        val currentAppImage = File(currentAppImagePath)
        if (!currentAppImage.exists() || !downloadedFile.exists()) return false

        return try {
            downloadedFile.setExecutable(true, false)
            try {
                Files.move(
                    downloadedFile.toPath(),
                    currentAppImage.toPath(),
                    StandardCopyOption.REPLACE_EXISTING,
                    StandardCopyOption.ATOMIC_MOVE
                )
            } catch (_: Throwable) {
                Files.copy(
                    downloadedFile.toPath(),
                    currentAppImage.toPath(),
                    StandardCopyOption.REPLACE_EXISTING
                )
                downloadedFile.delete()
            }
            currentAppImage.setExecutable(true, false)

            // Launch the updated AppImage in a separate detached process
            launcher(currentAppImage.absolutePath)
            true
        } catch (_: Throwable) {
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
