package com.emusync.ui.library.form

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetAccess
import com.emusync.domain.library.SaveTargetType
import com.emusync.domain.sync.remotePathSegments
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Keeps each save path, its chosen type and its cloud name aligned as rows are edited. */
internal class SaveTargetFormState(
    initialTargets: List<SaveTarget>,
    private val access: SaveTargetAccess,
) {
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
    }

    suspend fun refreshType(
        index: Int,
        expectedPath: String,
        ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
    ) {
        val type = withContext(ioDispatcher) { access.typeOf(expectedPath.trim()) } ?: return
        if (savePaths.getOrNull(index) == expectedPath) updateType(index, type)
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
                    access.typeOf(trimmed) ?: saveTypes.getOrElse(index) { SaveTargetType.FILE },
                    saveCloudPaths.getOrElse(index) { "" }.trim().ifBlank { null },
                )
            }
    }
}

private fun <T> List<T>.updated(index: Int, value: T): List<T> =
    toMutableList().apply { set(index, value) }
