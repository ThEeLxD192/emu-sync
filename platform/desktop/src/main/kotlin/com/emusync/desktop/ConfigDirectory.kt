package com.emusync.desktop

import java.awt.Desktop
import java.io.File

/** Reveals the directory containing the user configuration in the desktop file explorer. */
fun openConfigDirectory(configFile: File) {
    try {
        val dir = if (configFile.isDirectory) configFile else configFile.parentFile ?: configFile
        dir.mkdirs()
        if (Desktop.isDesktopSupported() && Desktop.getDesktop().isSupported(Desktop.Action.OPEN)) {
            Desktop.getDesktop().open(dir)
        } else {
            val os = System.getProperty("os.name", "").lowercase()
            when {
                os.contains("windows") -> ProcessBuilder("explorer", dir.absolutePath).start()
                os.contains("mac") -> ProcessBuilder("open", dir.absolutePath).start()
                else -> ProcessBuilder("xdg-open", dir.absolutePath).start()
            }
        }
    } catch (_: Exception) {
        // Ignored if headless or unsupported.
    }
}
