package com.emusync.steam

import java.io.*
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Represents a single Non-Steam Game shortcut entry from `shortcuts.vdf`.
 *
 * All fields mirror the binary VDF structure that Steam uses to store
 * non-Steam game shortcuts in `~/.local/share/Steam/userdata/<ID>/config/shortcuts.vdf`.
 */
data class SteamShortcut(
    val appId: Int,
    val appName: String,
    val exe: String,
    val startDir: String,
    val icon: String = "",
    val shortcutPath: String = "",
    val launchOptions: String = "",
    val isHidden: Boolean = false,
    val allowDesktopConfig: Boolean = true,
    val allowOverlay: Boolean = true,
    val openVR: Boolean = false,
    val devkit: Boolean = false,
    val devkitGameID: String = "",
    val devkitOverrideAppID: Int = 0,
    val lastPlayTime: Int = 0,
    val flatpakAppID: String = "",
    val sortAs: String = "",
    val tags: Map<String, String> = emptyMap(),
)

/**
 * Parser and writer for Steam's binary VDF (Valve Data Format) used by `shortcuts.vdf`.
 *
 * Binary format markers:
 * - `0x00` — Start of sub-object, followed by null-terminated name
 * - `0x01` — String value, followed by null-terminated key then null-terminated value
 * - `0x02` — Int32 value, followed by null-terminated key then 4 bytes little-endian
 * - `0x08` — End of current object
 *
 * File structure:
 * ```
 * 0x00 "shortcuts"
 *   0x00 "0"          ← first shortcut
 *     0x02 "appid" <4 bytes>
 *     0x01 "AppName" "My Game"
 *     ...
 *     0x00 "tags"
 *     0x08            ← end tags
 *   0x08              ← end shortcut 0
 *   0x00 "1"          ← second shortcut
 *     ...
 *   0x08              ← end shortcut 1
 * 0x08                ← end shortcuts
 * ```
 */
object BinaryVdf {

    private const val TYPE_OBJECT: Int = 0x00
    private const val TYPE_STRING: Int = 0x01
    private const val TYPE_INT32: Int = 0x02
    private const val TYPE_END: Int = 0x08

    /**
     * Reads a `shortcuts.vdf` file and returns all shortcut entries.
     *
     * @param file The `shortcuts.vdf` file to read.
     * @return List of [SteamShortcut] entries found in the file.
     * @throws IllegalArgumentException if the file format is invalid.
     */
    fun read(file: File): List<SteamShortcut> {
        if (!file.exists()) return emptyList()

        val bytes = file.readBytes()
        if (bytes.isEmpty()) return emptyList()

        val stream = ByteArrayInputStream(bytes)

        // Read root object marker and name ("shortcuts")
        val rootType = stream.read()
        require(rootType == TYPE_OBJECT) { "Expected object marker (0x00) at start, got 0x${rootType.toString(16)}" }
        val rootName = readNullTermString(stream)
        require(rootName == "shortcuts") { "Expected 'shortcuts' root object, got '$rootName'" }

        val shortcuts = mutableListOf<SteamShortcut>()

        // Read each shortcut entry (sub-objects named "0", "1", "2", ...)
        while (true) {
            val type = stream.read()
            if (type == TYPE_END || type == -1) break
            require(type == TYPE_OBJECT) { "Expected object (0x00) or end (0x08), got 0x${type.toString(16)}" }

            // Index name ("0", "1", ...) — we don't use it, just consume
            readNullTermString(stream)

            val fields = readObjectFields(stream)
            shortcuts.add(buildShortcut(fields))
        }

        return shortcuts
    }

    /**
     * Writes a list of shortcuts to a `shortcuts.vdf` file.
     * This completely replaces the file content — callers must include
     * ALL shortcuts (existing + new) in the list.
     *
     * @param file The target `shortcuts.vdf` file.
     * @param shortcuts All shortcuts to write.
     */
    fun write(file: File, shortcuts: List<SteamShortcut>) {
        val output = ByteArrayOutputStream()

        // Root object "shortcuts"
        output.write(TYPE_OBJECT)
        writeNullTermString(output, "shortcuts")

        // Each shortcut as a numbered sub-object
        shortcuts.forEachIndexed { index, shortcut ->
            output.write(TYPE_OBJECT)
            writeNullTermString(output, index.toString())
            writeShortcutFields(output, shortcut)
            output.write(TYPE_END) // end of this shortcut
        }

        output.write(TYPE_END) // end of "shortcuts"
        output.write(TYPE_END) // file-level terminator (required by Steam)

        file.parentFile?.mkdirs()
        file.writeBytes(output.toByteArray())
    }

    // ── Reading helpers ──────────────────────────────────────────────

    /**
     * Reads all fields within an object until TYPE_END (0x08) is encountered.
     * Returns a map of field names to values (String, Int, or nested Map).
     */
    @Suppress("UNCHECKED_CAST")
    private fun readObjectFields(stream: ByteArrayInputStream): Map<String, Any> {
        val fields = mutableMapOf<String, Any>()

        while (true) {
            val type = stream.read()
            when (type) {
                TYPE_END, -1 -> return fields
                TYPE_STRING -> {
                    val key = readNullTermString(stream)
                    val value = readNullTermString(stream)
                    fields[key] = value
                }
                TYPE_INT32 -> {
                    val key = readNullTermString(stream)
                    val value = readInt32LE(stream)
                    fields[key] = value
                }
                TYPE_OBJECT -> {
                    val key = readNullTermString(stream)
                    val subFields = readObjectFields(stream)
                    fields[key] = subFields
                }
                else -> error("Unknown VDF type marker: 0x${type.toString(16)}")
            }
        }
    }

    /** Reads a null-terminated string from the stream. */
    private fun readNullTermString(stream: ByteArrayInputStream): String {
        val buffer = ByteArrayOutputStream()
        while (true) {
            val b = stream.read()
            if (b == 0x00 || b == -1) break
            buffer.write(b)
        }
        return buffer.toString(Charsets.UTF_8.name())
    }

    /** Reads a 4-byte little-endian Int32. */
    private fun readInt32LE(stream: ByteArrayInputStream): Int {
        val bytes = ByteArray(4)
        stream.read(bytes)
        return ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).int
    }

    // ── Writing helpers ──────────────────────────────────────────────

    /** Writes all fields of a [SteamShortcut] in the expected binary order. */
    private fun writeShortcutFields(output: ByteArrayOutputStream, shortcut: SteamShortcut) {
        writeInt32Field(output, "appid", shortcut.appId)
        writeStringField(output, "AppName", shortcut.appName)
        writeStringField(output, "Exe", shortcut.exe)
        writeStringField(output, "StartDir", shortcut.startDir)
        writeStringField(output, "icon", shortcut.icon)
        writeStringField(output, "ShortcutPath", shortcut.shortcutPath)
        writeStringField(output, "LaunchOptions", shortcut.launchOptions)
        writeInt32Field(output, "IsHidden", if (shortcut.isHidden) 1 else 0)
        writeInt32Field(output, "AllowDesktopConfig", if (shortcut.allowDesktopConfig) 1 else 0)
        writeInt32Field(output, "AllowOverlay", if (shortcut.allowOverlay) 1 else 0)
        writeInt32Field(output, "OpenVR", if (shortcut.openVR) 1 else 0)
        writeInt32Field(output, "Devkit", if (shortcut.devkit) 1 else 0)
        writeStringField(output, "DevkitGameID", shortcut.devkitGameID)
        writeInt32Field(output, "DevkitOverrideAppID", shortcut.devkitOverrideAppID)
        writeInt32Field(output, "LastPlayTime", shortcut.lastPlayTime)
        writeStringField(output, "FlatpakAppID", shortcut.flatpakAppID)
        writeStringField(output, "sortas", shortcut.sortAs)

        // Tags sub-object
        output.write(TYPE_OBJECT)
        writeNullTermString(output, "tags")
        shortcut.tags.forEach { (key, value) ->
            writeStringField(output, key, value)
        }
        output.write(TYPE_END) // end of tags
    }

    /** Writes a string field: 0x01 + key\0 + value\0 */
    private fun writeStringField(output: ByteArrayOutputStream, key: String, value: String) {
        output.write(TYPE_STRING)
        writeNullTermString(output, key)
        writeNullTermString(output, value)
    }

    /** Writes an Int32 field: 0x02 + key\0 + 4 bytes LE */
    private fun writeInt32Field(output: ByteArrayOutputStream, key: String, value: Int) {
        output.write(TYPE_INT32)
        writeNullTermString(output, key)
        val bytes = ByteBuffer.allocate(4).order(ByteOrder.LITTLE_ENDIAN).putInt(value).array()
        output.write(bytes)
    }

    /** Writes a null-terminated UTF-8 string. */
    private fun writeNullTermString(output: ByteArrayOutputStream, value: String) {
        output.write(value.toByteArray(Charsets.UTF_8))
        output.write(0x00)
    }

    // ── Shortcut construction ────────────────────────────────────────

    /** Builds a [SteamShortcut] from a raw field map parsed from the binary. */
    @Suppress("UNCHECKED_CAST")
    private fun buildShortcut(fields: Map<String, Any>): SteamShortcut {
        return SteamShortcut(
            appId = (fields["appid"] as? Int) ?: 0,
            appName = (fields["AppName"] as? String) ?: (fields["appname"] as? String) ?: "",
            exe = (fields["Exe"] as? String) ?: (fields["exe"] as? String) ?: "",
            startDir = (fields["StartDir"] as? String) ?: (fields["startdir"] as? String) ?: "",
            icon = (fields["icon"] as? String) ?: "",
            shortcutPath = (fields["ShortcutPath"] as? String) ?: "",
            launchOptions = (fields["LaunchOptions"] as? String) ?: "",
            isHidden = ((fields["IsHidden"] as? Int) ?: 0) != 0,
            allowDesktopConfig = ((fields["AllowDesktopConfig"] as? Int) ?: 1) != 0,
            allowOverlay = ((fields["AllowOverlay"] as? Int) ?: 1) != 0,
            openVR = ((fields["OpenVR"] as? Int) ?: 0) != 0,
            devkit = ((fields["Devkit"] as? Int) ?: 0) != 0,
            devkitGameID = (fields["DevkitGameID"] as? String) ?: "",
            devkitOverrideAppID = (fields["DevkitOverrideAppID"] as? Int) ?: 0,
            lastPlayTime = (fields["LastPlayTime"] as? Int) ?: 0,
            flatpakAppID = (fields["FlatpakAppID"] as? String) ?: "",
            sortAs = (fields["sortas"] as? String) ?: "",
            tags = (fields["tags"] as? Map<String, String>) ?: emptyMap(),
        )
    }
}
