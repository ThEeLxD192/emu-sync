package com.emusync.update

import com.emusync.domain.update.UpdateFiles
import com.emusync.domain.update.UpdateInfo
import java.io.File
import java.nio.file.Files
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

internal class UpdateStagingFiles(
    private val manager: UpdateManager,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
) : UpdateFiles {
    override suspend fun prepareDestination(info: UpdateInfo): String {
        val windows = manager.isWindows()
        val extension =
            File(info.assetName).extension.ifBlank { if (windows) "msi" else "AppImage" }
        return withContext(ioDispatcher) {
            val currentParent =
                if (!windows)
                    manager.getCurrentAppImagePath()?.let { File(it).absoluteFile.parentFile }
                else null
            val directory =
                currentParent?.takeIf { it.canWrite() }
                    ?: File(System.getProperty("java.io.tmpdir"))
            Files.createTempFile(directory.toPath(), ".EmuSync-update-", ".$extension")
                .toFile()
                .absolutePath
        }
    }

    override fun size(path: String) = File(path).length()

    override fun delete(path: String) {
        File(path).delete()
    }
}
