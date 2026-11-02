package com.emusync.composition

import com.emusync.runner.GameRunner
import com.emusync.steam.DesktopSteamLibrary
import com.emusync.steam.SteamShortcutManager

internal fun composeGameRunner(manager: SteamShortcutManager = SteamShortcutManager()): GameRunner =
    GameRunner(DesktopSteamLibrary(manager))
