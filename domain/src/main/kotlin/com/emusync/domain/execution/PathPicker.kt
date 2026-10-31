package com.emusync.domain.execution

fun interface PathPicker {
    fun pickPath(label: String, initialPath: String, type: PickerType): String?
}
