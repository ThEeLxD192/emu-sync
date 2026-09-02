package com.emusync.config

import com.emusync.model.AppConfig
import kotlinx.serialization.json.Json
import java.io.File

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
class ConfigManager(private val configPath: String) {

    /**
     * Json instance configured for EmuSync's config format:
     * - `classDiscriminator = "type"` — matches the `"type": "emulator"` / `"type": "native"` field in JSON.
     * - `ignoreUnknownKeys = true` — forward-compatible; new fields won't break older versions.
     * - `prettyPrint = true` — readable output if we ever write config back to disk.
     */
    private val json = Json {
        classDiscriminator = "type"
        ignoreUnknownKeys = true
        prettyPrint = true
    }

    /**
     * Loads and deserializes `config.json` from [configPath].
     *
     * @return The parsed [AppConfig].
     * @throws java.io.FileNotFoundException if [configPath] does not exist.
     * @throws kotlinx.serialization.SerializationException if the JSON structure is invalid.
     */
    fun load(): AppConfig {
        val file = File(configPath)
        require(file.exists()) { "Configuration file not found: ${file.absolutePath}" }
        val content = file.readText()
        return json.decodeFromString<AppConfig>(content)
    }

    /**
     * Serializes the given [AppConfig] back to JSON and writes it to [configPath].
     * Useful for future features like updating `driveFileId` after a first sync.
     */
    fun save(config: AppConfig) {
        val content = json.encodeToString(AppConfig.serializer(), config)
        File(configPath).writeText(content)
    }
}
