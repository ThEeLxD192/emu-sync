package com.emusync.ui.library.components

import com.emusync.domain.library.GameEntry

internal fun partitionIntoUnits(entries: List<GameEntry>): List<SidebarUnit> {
    val units = mutableListOf<SidebarUnit>()
    val seenGroups = mutableSetOf<String>()

    for (entry in entries) {
        val groupName = entry.group?.trim()?.takeIf { it.isNotBlank() }
        if (groupName == null) {
            units.add(SidebarUnit.StandaloneUnit(entry))
        } else {
            val normalized = groupName.lowercase()
            if (seenGroups.add(normalized)) {
                val groupEntries = entries.filter {
                    it.group?.trim().equals(groupName, ignoreCase = true)
                }
                units.add(SidebarUnit.GroupUnit(groupName, groupEntries))
            }
        }
    }
    return units
}

internal fun flattenUnits(units: List<SidebarUnit>): List<GameEntry> {
    return units.flatMap { unit ->
        when (unit) {
            is SidebarUnit.GroupUnit -> unit.entries
            is SidebarUnit.StandaloneUnit -> listOf(unit.entry)
        }
    }
}

internal fun moveTopLevelUnit(
    entries: List<GameEntry>,
    fromUnitIndex: Int,
    toUnitIndex: Int,
): List<GameEntry> {
    val units = partitionIntoUnits(entries).toMutableList()
    if (
        fromUnitIndex !in units.indices ||
            toUnitIndex !in units.indices ||
            fromUnitIndex == toUnitIndex
    ) {
        return entries
    }
    val item = units.removeAt(fromUnitIndex)
    units.add(toUnitIndex, item)
    return flattenUnits(units)
}

internal fun moveChildEntry(
    entries: List<GameEntry>,
    groupName: String,
    fromSiblingIndex: Int,
    toSiblingIndex: Int,
): List<GameEntry> {
    val units = partitionIntoUnits(entries).toMutableList()
    val groupUnitIndex = units.indexOfFirst {
        it is SidebarUnit.GroupUnit && it.groupName.equals(groupName, ignoreCase = true)
    }
    if (groupUnitIndex == -1) return entries
    val groupUnit = units[groupUnitIndex] as SidebarUnit.GroupUnit
    val siblingEntries = groupUnit.entries.toMutableList()
    return if (
        fromSiblingIndex !in siblingEntries.indices ||
            toSiblingIndex !in siblingEntries.indices ||
            fromSiblingIndex == toSiblingIndex
    )
        entries
    else {
        val moved = siblingEntries.removeAt(fromSiblingIndex)
        siblingEntries.add(toSiblingIndex, moved)
        units[groupUnitIndex] = groupUnit.copy(entries = siblingEntries)
        flattenUnits(units)
    }
}

internal fun computeSidebarRows(
    entries: List<GameEntry>,
    collapsedGroups: Set<String>,
    selectedEntry: GameEntry?,
): List<SidebarRow> {
    val rows = mutableListOf<SidebarRow>()
    val units = partitionIntoUnits(entries)
    val totalUnits = units.size

    units.forEachIndexed { unitIndex, unit ->
        when (unit) {
            is SidebarUnit.StandaloneUnit -> {
                rows.add(
                    SidebarRow.EntryRow(
                        entry = unit.entry,
                        isNested = false,
                        groupName = null,
                        siblingIndex = 0,
                        totalSiblings = 1,
                        unitIndex = unitIndex,
                        totalUnits = totalUnits,
                    )
                )
            }
            is SidebarUnit.GroupUnit -> {
                val groupEntries = unit.entries
                val isCollapsed = collapsedGroups.any {
                    it.equals(unit.groupName, ignoreCase = true)
                }
                val hasSelectedChild =
                    selectedEntry != null && groupEntries.any { it.name == selectedEntry.name }

                rows.add(
                    SidebarRow.FolderHeader(
                        groupName = unit.groupName,
                        count = groupEntries.size,
                        isCollapsed = isCollapsed,
                        hasSelectedChild = hasSelectedChild,
                        firstChild = groupEntries.firstOrNull(),
                        unitIndex = unitIndex,
                        totalUnits = totalUnits,
                    )
                )

                if (!isCollapsed) {
                    val totalSiblings = groupEntries.size
                    groupEntries.forEachIndexed { siblingIndex, groupEntry ->
                        rows.add(
                            SidebarRow.EntryRow(
                                entry = groupEntry,
                                isNested = true,
                                groupName = unit.groupName,
                                siblingIndex = siblingIndex,
                                totalSiblings = totalSiblings,
                                unitIndex = unitIndex,
                                totalUnits = totalUnits,
                            )
                        )
                    }
                }
            }
        }
    }
    return rows
}

internal val SidebarRow.key: String
    get() =
        when (this) {
            is SidebarRow.FolderHeader -> "folder_${groupName.lowercase()}"
            is SidebarRow.EntryRow -> "entry_${entry.name}"
        }

internal fun moveSidebarEntry(
    entries: List<GameEntry>,
    row: SidebarRow.EntryRow,
    direction: Int,
): List<GameEntry> =
    if (row.isNested && row.groupName != null)
        moveChildEntry(entries, row.groupName, row.siblingIndex, row.siblingIndex + direction)
    else moveTopLevelUnit(entries, row.unitIndex, row.unitIndex + direction)
