package com.emusync.steam

import java.io.File
import kotlin.test.assertEquals
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class WindowsSteamPathsTest {
    @Test
    fun `user registry location avoids consulting machine registry`(@TempDir directory: File) {
        val userdata = File(directory, "userdata").apply { mkdir() }
        val paths = WindowsSteamPaths { key, value ->
            assertEquals("HKCU\\Software\\Valve\\Steam", key)
            assertEquals("SteamPath", value)
            directory.path
        }

        assertEquals(userdata, paths.find())
    }

    @Test
    fun `existing machine userdata precedes an uninitialized user install`(
        @TempDir directory: File
    ) {
        val userRoot = File(directory, "user").apply { mkdir() }
        val machineRoot = File(directory, "machine").apply { mkdir() }
        val userdata = File(machineRoot, "userdata").apply { mkdir() }
        val queries = mutableListOf<String>()
        val paths = WindowsSteamPaths { key, _ ->
            queries += key
            when (key) {
                "HKCU\\Software\\Valve\\Steam" -> userRoot.path
                "HKLM\\SOFTWARE\\WOW6432Node\\Valve\\Steam" -> machineRoot.path
                else -> error("The first machine registry location already resolved userdata")
            }
        }

        assertEquals(userdata, paths.find())
        assertEquals(2, queries.size)
    }

    @Test
    fun `native machine registry is checked after missing redirected registry`(
        @TempDir directory: File
    ) {
        val userdata = File(directory, "userdata").apply { mkdir() }
        val queries = mutableListOf<String>()
        val paths = WindowsSteamPaths { key, _ ->
            queries += key
            if (key == "HKLM\\SOFTWARE\\Valve\\Steam") directory.path else null
        }

        assertEquals(userdata, paths.find())
        assertEquals(
            listOf(
                "HKCU\\Software\\Valve\\Steam",
                "HKLM\\SOFTWARE\\WOW6432Node\\Valve\\Steam",
                "HKLM\\SOFTWARE\\Valve\\Steam",
            ),
            queries,
        )
    }
}
