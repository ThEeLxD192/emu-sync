package com.emusync.update

import com.emusync.domain.update.UpdateInfo
import com.emusync.domain.update.UpdateSource
import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.get
import io.ktor.client.request.header
import io.ktor.client.statement.HttpResponse
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import java.io.File
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Borrows its HTTP client; the composition owns and closes it. */
class GitHubUpdateSource(
    private val client: HttpClient,
    private val owner: String,
    private val repo: String,
    private val currentVersion: String,
    private val isWindows: Boolean,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
) : UpdateSource {
    private val downloader = UpdateDownloader(client, ioDispatcher)

    override suspend fun download(
        info: UpdateInfo,
        destination: String,
        onProgress: (Float) -> Unit,
    ): Result<Unit> = downloader.download(info.downloadUrl, File(destination), onProgress)

    private fun findBestAsset(assets: List<GitHubAsset>): GitHubAsset? =
        findBestAssetForPlatform(assets, isWindows)

    /** Checks GitHub for the latest release and compares with the running version. */
    override suspend fun check(): UpdateInfo? =
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

    companion object {
        /**
         * Selects the most appropriate asset from a release based on platform priority. Windows:
         * .msi > .exe > .zip Linux: .AppImage > .tar.gz > .deb
         */
        internal fun findBestAssetForPlatform(
            assets: List<GitHubAsset>,
            isWindows: Boolean,
        ): GitHubAsset? {
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
        internal fun isNewerVersion(current: String, remote: String): Boolean {
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
