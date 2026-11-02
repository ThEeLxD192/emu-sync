package com.emusync.ui.library.components

import com.emusync.domain.library.GameEntry

sealed interface SidebarUnit {
    data class GroupUnit(val groupName: String, val entries: List<GameEntry>) : SidebarUnit

    data class StandaloneUnit(val entry: GameEntry) : SidebarUnit
}
