package com.emusync.ui.library.components

import com.emusync.domain.library.GameEntry

sealed interface SidebarRow {
    data class FolderHeader(
        val groupName: String,
        val count: Int,
        val isCollapsed: Boolean,
        val hasSelectedChild: Boolean,
        val firstChild: GameEntry?,
        val unitIndex: Int,
        val totalUnits: Int,
    ) : SidebarRow

    data class EntryRow(
        val entry: GameEntry,
        val isNested: Boolean,
        val groupName: String?,
        val siblingIndex: Int,
        val totalSiblings: Int,
        val unitIndex: Int,
        val totalUnits: Int,
    ) : SidebarRow
}
