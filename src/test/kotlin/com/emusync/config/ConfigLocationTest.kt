package com.emusync.config

import java.io.File
import kotlin.test.assertEquals
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class ConfigLocationTest {
    @Test
    fun `explicit overrides preserve priority without consulting lower priority providers`(
        @TempDir directory: File
    ) {
        val custom = File(directory, "custom.json")
        val property = File(directory, "property.json")
        val environment = File(directory, "environment.json")

        assertEquals(
            custom,
            ConfigManager.resolveDefaultConfigFile(
                customPath = custom.path,
                propertyProvider = { error("A custom path must not consult properties") },
                envProvider = { error("A custom path must not consult the environment") },
            ),
        )
        assertEquals(
            property,
            ConfigManager.resolveDefaultConfigFile(
                customPath = " ",
                propertyProvider = { key ->
                    check(key == "emusync.config.path")
                    property.path
                },
                envProvider = { error("A property override must not consult the environment") },
            ),
        )
        assertEquals(
            environment,
            ConfigManager.resolveDefaultConfigFile(
                propertyProvider = { key ->
                    check(key == "emusync.config.path")
                    " "
                },
                envProvider = { key ->
                    check(key == "EMUSYNC_CONFIG")
                    environment.path
                },
            ),
        )
    }

    @Test
    fun `AppImage configuration precedes working directory and missing portable config falls back`(
        @TempDir directory: File
    ) {
        val portable = File(directory, "portable").apply { mkdir() }
        val working = File(directory, "working").apply { mkdir() }
        val portableConfig = File(portable, "config.json").apply { writeText("{}") }
        val workingConfig = File(working, "config.json").apply { writeText("{}") }
        val appImage = File(portable, "EmuSync.AppImage")
        val envProvider: (String) -> String? = { key ->
            when (key) {
                "EMUSYNC_CONFIG" -> null
                "APPIMAGE" -> appImage.path
                else -> error("Portable configuration must not consult platform settings")
            }
        }
        val properties: (String) -> String? = { key ->
            check(key == "emusync.config.path")
            null
        }

        assertEquals(
            portableConfig,
            ConfigManager.resolveDefaultConfigFile(
                envProvider = envProvider,
                propertyProvider = properties,
                workingDir = working,
            ),
        )
        check(portableConfig.delete())
        assertEquals(
            workingConfig,
            ConfigManager.resolveDefaultConfigFile(
                envProvider = envProvider,
                propertyProvider = properties,
                workingDir = working,
            ),
        )
    }

    @Test
    fun `absent blank or parentless AppImage path preserves working directory fallback`(
        @TempDir directory: File
    ) {
        val config = File(directory, "config.json").apply { writeText("{}") }
        for (appImage in listOf(null, " ", "EmuSync.AppImage")) {
            assertEquals(
                config,
                ConfigManager.resolveDefaultConfigFile(
                    envProvider = { if (it == "APPIMAGE") appImage else null },
                    propertyProvider = { null },
                    workingDir = directory,
                ),
            )
        }
    }

    @Test
    fun `missing portable and working configurations use the platform directory`(
        @TempDir directory: File
    ) {
        val xdg = File(directory, "xdg")
        val expected = File(xdg, "emusync/config.json")

        assertEquals(
            expected,
            ConfigManager.resolveDefaultConfigFile(
                envProvider = { if (it == "XDG_CONFIG_HOME") xdg.path else null },
                propertyProvider = { if (it == "os.name") "Linux" else null },
                workingDir = directory,
            ),
        )
        assertEquals(
            File("", ".config/emusync/config.json"),
            ConfigManager.getDefaultPlatformConfigFile(
                envProvider = { null },
                propertyProvider = { null },
            ),
        )
    }
}
