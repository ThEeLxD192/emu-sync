package com.emusync.steam

import java.io.File

/** Keeps registry and fallback precedence independent of shortcut registration. */
internal class WindowsSteamPaths(private val registryQuery: (String, String) -> String?) {
    fun find(): File? {
        val userPath = registryQuery("HKCU\\Software\\Valve\\Steam", "SteamPath")
        return existingUserdata(userPath)
            ?: run {
                val machinePath =
                    registryQuery("HKLM\\SOFTWARE\\WOW6432Node\\Valve\\Steam", "InstallPath")
                        ?: registryQuery("HKLM\\SOFTWARE\\Valve\\Steam", "InstallPath")
                existingUserdata(machinePath) ?: fallback(userPath, machinePath)
            }
    }

    private fun existingUserdata(path: String?): File? =
        installRoot(path)?.let { File(it, "userdata") }?.takeIf { it.exists() && it.isDirectory }

    private fun installRoot(path: String?): File? =
        path?.takeIf { it.isNotBlank() }?.let { File(it.replace('/', File.separatorChar)) }

    private fun fallback(userPath: String?, machinePath: String?): File? {
        val candidates =
            listOfNotNull(
                System.getenv("ProgramFiles(x86)")?.let { File(it, "Steam/userdata") },
                System.getenv("ProgramFiles")?.let { File(it, "Steam/userdata") },
                File("C:\\Program Files (x86)\\Steam\\userdata"),
                File("C:\\Program Files\\Steam\\userdata"),
                File("C:\\Steam\\userdata"),
                File("D:\\Steam\\userdata"),
            )
        return candidates.firstOrNull { it.exists() && it.isDirectory }
            ?: installRoot(userPath)?.takeIf { it.exists() }?.let { File(it, "userdata") }
            ?: installRoot(machinePath)?.takeIf { it.exists() }?.let { File(it, "userdata") }
            ?: candidates.firstOrNull()
    }
}
