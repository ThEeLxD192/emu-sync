package com.emusync.util

import java.io.File

/** Removes AppImage runtime variables before starting unrelated child applications. */
object AppImageEnvironment {
    fun clean(environment: MutableMap<String, String>) {
        val appDir = environment["APPDIR"]?.trimEnd(File.separatorChar).orEmpty()
        environment.remove("APPDIR")
        environment.remove("ARGV0")
        environment.remove("OWD")

        val ldLibraryPath = environment["LD_LIBRARY_PATH"] ?: return
        val cleanedPaths = ldLibraryPath.split(File.pathSeparator).filterNot { path ->
            (appDir.isNotEmpty() && (path == appDir || path.startsWith("$appDir${File.separator}"))) ||
                path.split('/', '\\').any { segment -> segment.startsWith(".mount_") }
        }
        if (cleanedPaths.isEmpty()) {
            environment.remove("LD_LIBRARY_PATH")
        } else {
            environment["LD_LIBRARY_PATH"] = cleanedPaths.joinToString(File.pathSeparator)
        }
    }
}
