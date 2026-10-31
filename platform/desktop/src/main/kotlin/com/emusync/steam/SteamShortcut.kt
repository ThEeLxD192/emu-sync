package com.emusync.steam

/**
 * Represents a single Non-Steam Game shortcut entry from `shortcuts.vdf`.
 *
 * All fields mirror the binary VDF structure that Steam uses to store non-Steam game shortcuts in
 * `~/.local/share/Steam/userdata/<ID>/config/shortcuts.vdf`.
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
    internal val extraFields: Map<String, Any> = emptyMap(),
)
