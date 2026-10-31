package com.emusync.ui.dialogs

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetType
import com.emusync.domain.sync.remotePathSegments
import java.io.File

/** Keeps each save path, its chosen type and its cloud name aligned as rows are edited. */
internal class SaveTargetFormState(initialTargets: List<SaveTarget>) {
    var savePaths by mutableStateOf(initialTargets.map { it.path }.ifEmpty { listOf("") })
        private set

    var saveTypes by
        mutableStateOf(initialTargets.map { it.type }.ifEmpty { listOf(SaveTargetType.FILE) })
        private set

    var saveCloudPaths by
        mutableStateOf(initialTargets.map { it.cloudPath.orEmpty() }.ifEmpty { listOf("") })
        private set

    val cloudPathsAreValid: Boolean
        get() = saveCloudPaths.all { it.isBlank() || remotePathSegments(it.trim()) != null }

    fun updatePath(index: Int, value: String) {
        savePaths = savePaths.updated(index, value)
        saveTypes = saveTypes.updated(index, formTargetType(value.trim(), saveTypes[index]))
    }

    fun updateType(index: Int, type: SaveTargetType) {
        saveTypes = saveTypes.updated(index, type)
    }

    fun toggleType(index: Int) {
        updateType(
            index,
            if (saveTypes[index] == SaveTargetType.FILE) SaveTargetType.DIRECTORY
            else SaveTargetType.FILE,
        )
    }

    fun updateCloudPath(index: Int, value: String) {
        saveCloudPaths = saveCloudPaths.updated(index, value)
    }

    fun addPath() {
        savePaths = savePaths + ""
        saveTypes = saveTypes + SaveTargetType.FILE
        saveCloudPaths = saveCloudPaths + ""
    }

    fun removePath(index: Int) {
        savePaths = savePaths.toMutableList().apply { removeAt(index) }
        saveTypes = saveTypes.toMutableList().apply { removeAt(index) }
        saveCloudPaths = saveCloudPaths.toMutableList().apply { removeAt(index) }
    }

    fun buildTargets(): List<SaveTarget> = savePaths.mapIndexedNotNull { index, path ->
        path
            .trim()
            .takeIf { it.isNotBlank() }
            ?.let { trimmed ->
                SaveTarget(
                    trimmed,
                    formTargetType(trimmed, saveTypes.getOrElse(index) { SaveTargetType.FILE }),
                    saveCloudPaths.getOrElse(index) { "" }.trim().ifBlank { null },
                )
            }
    }
}

private fun formTargetType(path: String, fallback: SaveTargetType): SaveTargetType {
    val file = File(path)
    return if (file.exists()) {
        if (file.isDirectory) SaveTargetType.DIRECTORY else SaveTargetType.FILE
    } else fallback
}

private fun <T> List<T>.updated(index: Int, value: T): List<T> =
    toMutableList().apply { set(index, value) }
