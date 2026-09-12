package com.emusync.update

import com.emusync.AppInfo
import io.ktor.client.*
import io.ktor.client.call.*
import io.ktor.client.engine.cio.*
import io.ktor.client.plugins.contentnegotiation.*
import io.ktor.client.request.*
import io.ktor.client.statement.*
import io.ktor.http.*
import io.ktor.serialization.kotlinx.json.*
import io.ktor.utils.io.*
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
)

class UpdateManager(
    private val client: HttpClient = HttpClient(CIO) {
        install(ContentNegotiation) {
            json(Json {
                ignoreUnknownKeys = true
                isLenient = true
            })
        }
    }
) {
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

            // Find an AppImage asset (e.g. EmuSync-x86_64.AppImage or any .AppImage)
            val asset = release.assets.firstOrNull {
                it.name.endsWith(".AppImage", ignoreCase = true)
            } ?: return@withContext null

            UpdateInfo(
                version = remoteVersion,
                tagName = release.tagName,
                releaseNotes = release.body ?: release.name ?: "New version $remoteVersion available.",
                downloadUrl = asset.downloadUrl,
                assetSize = asset.size,
            )
        } catch (_: Throwable) {
            null
        }
    }

    /**
     * Downloads the AppImage from [downloadUrl] to [destinationFile] reporting progress.
     */
    suspend fun downloadUpdate(
        downloadUrl: String,
        destinationFile: File,
        onProgress: (Float) -> Unit,
    ): Boolean = withContext(Dispatchers.IO) {
        try {
            destinationFile.parentFile?.mkdirs()
            val response: HttpResponse = client.get(downloadUrl) {
                header(HttpHeaders.UserAgent, "EmuSync-Desktop-App")
            }

            if (response.status != HttpStatusCode.OK) return@withContext false

            val totalBytes = response.contentLength() ?: -1L
            val channel: ByteReadChannel = response.body()
            var bytesRead = 0L

            destinationFile.outputStream().use { output ->
                val buffer = ByteArray(32 * 1024)
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
            true
        } catch (_: Throwable) {
            destinationFile.delete()
            false
        }
    }

    /**
     * Replaces the currently running AppImage and restarts the application.
     */
    fun applyUpdateAndRestart(downloadedFile: File): Boolean {
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
                Files.move(
                    downloadedFile.toPath(),
                    currentAppImage.toPath(),
                    StandardCopyOption.REPLACE_EXISTING
                )
            }

            // Launch the updated AppImage in a separate detached process
            ProcessBuilder(currentAppImage.absolutePath)
                .inheritIO()
                .start()

            // Exit current instance cleanly
            exitProcess(0)
        } catch (_: Throwable) {
            false
        }
    }

    companion object {
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
