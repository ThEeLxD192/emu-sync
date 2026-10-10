package com.emusync.config

import com.emusync.model.AppConfig
import com.emusync.model.migrateLegacySaveTargets
import kotlinx.serialization.json.Json
import java.io.File
import java.nio.channels.FileChannel
import java.nio.file.AtomicMoveNotSupportedException
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.StandardCopyOption
import java.nio.file.StandardOpenOption
import java.nio.file.attribute.PosixFilePermission

/**
 * Manages loading and providing access to the application configuration.
 *
 * Reads a `config.json` file and deserializes it into an [AppConfig] object,
 * handling polymorphic deserialization of [GameEntry] subtypes via the `"type"` discriminator.
 *
 * Usage:
 * ```
 * val manager = ConfigManager("./config.json")
 * val config = manager.load()
 * config.entries.forEach { println(it.name) }
 * ```
 */
class ConfigManager(
    val configFile: File = resolveDefaultConfigFile(),
    private val beforeReplace: (Path) -> Unit = {},
) {
    /** Secondary constructor accepting a file path string. */
    constructor(configPath: String) : this(File(configPath))

    val configPath: String
        get() = configFile.absolutePath

    /**
     * Json instance configured for EmuSync's config format:
     * - `classDiscriminator = "type"` — matches the `"type": "emulator"` / `"type": "native"` field in JSON.
     * - `ignoreUnknownKeys = true` — forward-compatible; new fields won't break older versions.
     * - `prettyPrint = true` — readable output when config is written back to disk.
     */
    private val json = Json {
        classDiscriminator = "type"
        ignoreUnknownKeys = true
        prettyPrint = true
    }

    /**
     * Loads and deserializes `config.json` from [configFile].
     *
     * If [configFile] does not exist (or is empty), and [autoCreate] is true,
     * it generates a basic configuration, writes it to disk, and returns it.
     *
     * @param autoCreate If true (default), creates a basic config if file is missing or empty.
     * @return The parsed [AppConfig].
     * @throws IllegalArgumentException if file does not exist and [autoCreate] is false.
     * @throws kotlinx.serialization.SerializationException if the JSON structure is invalid.
     */
    fun load(autoCreate: Boolean = true): AppConfig {
        if (!configFile.exists() || configFile.length() == 0L) {
            if (autoCreate) {
                val defaultConfig = createDefaultConfig()
                save(defaultConfig)
                return defaultConfig
            } else {
                throw IllegalArgumentException("Configuration file not found: ${configFile.absolutePath}")
            }
        }
        val content = configFile.readText()
        val decoded = json.decodeFromString<AppConfig>(content)
        val migrated = decoded.copy(entries = decoded.entries.map { it.migrateLegacySaveTargets() })
        if (migrated != decoded) save(migrated)
        return migrated
    }

    /**
     * Serializes the given [AppConfig] back to JSON and writes it to [configFile].
     * Automatically creates parent directories if they do not exist.
     */
    fun save(config: AppConfig) {
        val absoluteFile = configFile.absoluteFile
        val parent = absoluteFile.parentFile
            ?: throw IllegalStateException("Could not resolve configuration directory")
        if (!parent.exists() && !parent.mkdirs()) {
            throw IllegalStateException("Could not create configuration directory: $parent")
        }
        val content = json.encodeToString(AppConfig.serializer(), config)
        var temporary: Path? = null
        try {
            temporary = Files.createTempFile(parent.toPath(), ".${configFile.name}.", ".tmp")
            Files.writeString(temporary, content, StandardOpenOption.TRUNCATE_EXISTING)
            restrictToCurrentUser(temporary)
            FileChannel.open(temporary, StandardOpenOption.WRITE).use { it.force(true) }
            beforeReplace(temporary)
            try {
                Files.move(
                    temporary,
                    absoluteFile.toPath(),
                    StandardCopyOption.ATOMIC_MOVE,
                    StandardCopyOption.REPLACE_EXISTING,
                )
            } catch (_: AtomicMoveNotSupportedException) {
                Files.move(temporary, absoluteFile.toPath(), StandardCopyOption.REPLACE_EXISTING)
            }
            temporary = null
            restrictToCurrentUser(absoluteFile.toPath())
        } finally {
            temporary?.let { Files.deleteIfExists(it) }
        }
    }

    private fun restrictToCurrentUser(path: Path) {
        try {
            Files.setPosixFilePermissions(
                path,
                setOf(PosixFilePermission.OWNER_READ, PosixFilePermission.OWNER_WRITE),
            )
        } catch (_: UnsupportedOperationException) {
            // Windows applies ACLs rather than POSIX modes; the user's config directory is used there.
        }
    }

    companion object {
        /**
         * Returns a basic, minimal [AppConfig] with empty entries and no Google Drive credentials.
         */
        fun createDefaultConfig(): AppConfig = AppConfig(
            googleDrive = null,
            entries = emptyList()
        )

        /**
         * Resolves the default configuration file location based on environment and platform.
         * Priority:
         * 1. Explicit override via [customPath], `emusync.config.path` property, or `EMUSYNC_CONFIG` env var.
         * 2. Co-located portable config:
         *    a) Next to the running `.AppImage` (via `APPIMAGE` env var) if `config.json` exists there.
         *    b) In the current working directory (`./config.json`) if it exists and is writable.
         * 3. Fallback to platform-standard user config directory:
         *    - Windows: %APPDATA%\emusync\config.json
         *    - Linux: ${XDG_CONFIG_HOME:-~/.config}/emusync/config.json
         *    - macOS: ~/Library/Application Support/emusync/config.json
         */
        fun resolveDefaultConfigFile(
            customPath: String? = null,
            envProvider: (String) -> String? = { System.getenv(it) },
            propertyProvider: (String) -> String? = { System.getProperty(it) },
            workingDir: File = File("."),
        ): File {
            // 1. Explicit argument or system property / env variable
            val explicit = customPath?.takeIf { it.isNotBlank() }
                ?: propertyProvider("emusync.config.path")?.takeIf { it.isNotBlank() }
                ?: envProvider("EMUSYNC_CONFIG")?.takeIf { it.isNotBlank() }
            if (explicit != null) {
                return File(explicit)
            }

            // 2. Co-located portable config:
            // a) If running as an AppImage on Linux, check if config.json exists in the same folder as the AppImage
            val appImagePath = envProvider("APPIMAGE")?.takeIf { it.isNotBlank() }
            if (appImagePath != null) {
                val appImageDir = File(appImagePath).parentFile
                if (appImageDir != null) {
                    val appImageConfig = File(appImageDir, "config.json")
                    if (appImageConfig.exists() && (appImageConfig.canWrite() || appImageDir.canWrite())) {
                        return appImageConfig
                    }
                }
            }

            // b) Check if config.json exists in working directory and is writable
            val localConfig = File(workingDir, "config.json")
            if (localConfig.exists() && (localConfig.canWrite() || workingDir.canWrite())) {
                return localConfig
            }

            // 3. Fallback to platform-standard user config directory
            return getDefaultPlatformConfigFile(envProvider, propertyProvider)
        }

        /**
         * Returns the standard OS user configuration file location:
         * - Windows: %APPDATA%\emusync\config.json
         * - Linux: ${XDG_CONFIG_HOME:-~/.config}/emusync/config.json
         * - macOS: ~/Library/Application Support/emusync/config.json
         */
        fun getDefaultPlatformConfigFile(
            envProvider: (String) -> String? = { System.getenv(it) },
            propertyProvider: (String) -> String? = { System.getProperty(it) },
        ): File {
            val osName = propertyProvider("os.name")?.lowercase() ?: ""
            val userHome = propertyProvider("user.home") ?: ""

            return when {
                osName.contains("windows") -> {
                    val appData = envProvider("APPDATA")?.takeIf { it.isNotBlank() }
                    val dir = if (appData != null) {
                        File(appData, "emusync")
                    } else {
                        File(userHome, "AppData/Roaming/emusync")
                    }
                    File(dir, "config.json")
                }
                osName.contains("mac") -> {
                    File(userHome, "Library/Application Support/emusync/config.json")
                }
                else -> {
                    val xdgConfig = envProvider("XDG_CONFIG_HOME")?.takeIf { it.isNotBlank() }
                    val dir = if (xdgConfig != null) {
                        File(xdgConfig, "emusync")
                    } else {
                        File(userHome, ".config/emusync")
                    }
                    File(dir, "config.json")
                }
            }
        }
    }
}
