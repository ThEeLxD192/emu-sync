package com.emusync.runner

import java.io.File

/** Reads process identities and supplies platform fallbacks when enumeration is unavailable. */
internal class ProcessMonitor {
    /**
     * Checks if Steam's Vulkan shader pre-compilation tool (fossilize_replay) is currently active.
     */
    internal fun isSteamShaderCompiling(): Boolean {
        return isProcessRunning("fossilize_replay") || isProcessRunning("fossilize_replay64")
    }

    /**
     * Checks if a process with [name] is currently running on the system.
     *
     * Uses Java 9+ [ProcessHandle] as the primary cross-platform mechanism (Windows, Linux, macOS).
     * Falls back to platform-specific CLI tools (tasklist on Windows, pgrep on Linux/macOS) if
     * needed.
     */
    internal fun isProcessRunning(name: String): Boolean {
        val cleanName = name.trim().removeSurrounding("\"")
        if (cleanName.isBlank()) return false
        return try {
            matchingProcessIds(cleanName).isNotEmpty()
        } catch (_: Exception) {
            fallbackProcessCheck(cleanName)
        }
    }

    internal fun matchingProcessIds(name: String): Set<Long> {
        val cleanName = File(name.trim().removeSurrounding("\"")).name
        if (cleanName.isBlank()) return emptySet()
        val targetWithoutExtension = File(cleanName).nameWithoutExtension

        return ProcessHandle.allProcesses().use { handles ->
            handles
                .filter {
                    it.isAlive &&
                        isActuallyAlive(it) &&
                        processMatches(it, cleanName, targetWithoutExtension)
                }
                .map { it.pid() }
                .toList()
                .toSet()
        }
    }

    private fun processMatches(
        handle: ProcessHandle,
        targetName: String,
        targetWithoutExtension: String,
    ): Boolean {
        val info = handle.info()
        val commandName = info.command().orElse(null)?.let { File(it).name }
        if (
            commandName != null &&
                (commandName.equals(targetName, ignoreCase = true) ||
                    File(commandName)
                        .nameWithoutExtension
                        .equals(targetWithoutExtension, ignoreCase = true))
        )
            return true

        return info.arguments().orElse(emptyArray()).any { argument ->
            val argumentName = File(argument.trim().removeSurrounding("\"")).name
            argumentName.equals(targetName, ignoreCase = true) ||
                File(argumentName)
                    .nameWithoutExtension
                    .equals(targetWithoutExtension, ignoreCase = true)
        }
    }

    private fun fallbackProcessCheck(cleanName: String): Boolean {
        val os = System.getProperty("os.name", "").lowercase()
        return try {
            if (os.contains("win")) {
                val targetExe =
                    if (cleanName.endsWith(".exe", ignoreCase = true)) cleanName
                    else "$cleanName.exe"
                val pb = ProcessBuilder("tasklist", "/FI", "IMAGENAME eq $targetExe", "/NH")
                val process = pb.start()
                val output = process.inputStream.bufferedReader().readText()
                process.waitFor()
                output.contains(targetExe, ignoreCase = true)
            } else {
                val process =
                    ProcessBuilder("pgrep", "-x", File(cleanName).nameWithoutExtension).start()
                process.waitFor() == 0
            }
        } catch (_: Exception) {
            false
        }
    }
}

internal fun isActuallyAlive(handle: ProcessHandle): Boolean {
    val os = System.getProperty("os.name", "").lowercase()
    if (!os.contains("linux")) return handle.isAlive
    val statusFile = File("/proc/${handle.pid()}/status")
    return if (!statusFile.exists()) {
        false
    } else {
        try {
            val state =
                statusFile
                    .useLines { lines -> lines.firstOrNull { it.startsWith("State:") } }
                    ?.substringAfter("State:")
                    ?.trim()
                    ?.firstOrNull()
            state != null && state != 'Z' && state != 'X'
        } catch (_: Exception) {
            false
        }
    }
}
