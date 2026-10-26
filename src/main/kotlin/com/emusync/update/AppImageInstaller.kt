package com.emusync.update

import java.io.File
import java.io.IOException
import java.nio.channels.FileChannel
import java.nio.file.AtomicMoveNotSupportedException
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.StandardCopyOption
import java.nio.file.StandardOpenOption

/** Stages updates on the destination filesystem and restores the old executable on failure. */
internal class AppImageInstaller(
    private val copyFile: (Path, Path) -> Unit = { source, destination ->
        Files.copy(source, destination, StandardCopyOption.REPLACE_EXISTING)
    },
    private val moveFile: (Path, Path) -> Unit = ::moveReplacing,
) {
    fun replace(source: File, target: File, onInstalled: () -> Unit = {}) {
        val sourcePath = source.toPath().toAbsolutePath().normalize()
        val targetPath = target.toPath().toAbsolutePath().normalize()
        require(Files.isRegularFile(sourcePath)) { "The downloaded AppImage is not a regular file: $source" }
        require(Files.isRegularFile(targetPath)) { "The current AppImage is not a regular file: $target" }
        require(!Files.isSameFile(sourcePath, targetPath)) { "The update must be a separate file" }

        val parent = targetPath.parent
        val staged = Files.createTempFile(parent, ".${target.name}.update.", ".tmp")
        var backup: Path? = null
        var originalBackedUp = false
        var completed = false
        try {
            // Copy before touching the current executable, including for cross-device downloads.
            copyFile(sourcePath, staged)
            if (!staged.toFile().setExecutable(true, false)) {
                throw IOException("Could not make the staged AppImage executable")
            }
            FileChannel.open(staged, StandardOpenOption.WRITE).use { it.force(true) }

            backup = Files.createTempFile(parent, ".${target.name}.old.", ".bak")
            moveFile(targetPath, backup)
            originalBackedUp = true
            moveFile(staged, targetPath)
            onInstalled()
            completed = true
        } catch (failure: Exception) {
            if (originalBackedUp) {
                try {
                    moveFile(requireNotNull(backup), targetPath)
                    originalBackedUp = false
                } catch (restoreFailure: Exception) {
                    failure.addSuppressed(IOException("The original AppImage is preserved at $backup", restoreFailure))
                }
            }
            throw failure
        } finally {
            // Never remove the download or a recovery backup after an unsuccessful installation.
            staged.toFile().delete()
            if (completed) {
                sourcePath.toFile().delete()
                backup?.toFile()?.delete()
            } else if (!originalBackedUp) {
                backup?.toFile()?.delete()
            }
        }
    }
}

private fun moveReplacing(source: Path, target: Path) {
    try {
        Files.move(source, target, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
    } catch (_: AtomicMoveNotSupportedException) {
        Files.move(source, target, StandardCopyOption.REPLACE_EXISTING)
    }
}
