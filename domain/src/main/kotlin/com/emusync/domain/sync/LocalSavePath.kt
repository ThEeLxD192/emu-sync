package com.emusync.domain.sync

/** Observations and safe resolution of one local save path, supplied by local storage. */
interface LocalSavePath {
    val path: String
    val name: String
    val absolutePath: String
    val normalizedPath: String
    val ancestorPaths: Sequence<String>
    val isFile: Boolean
    val isDirectory: Boolean

    fun exists(): Boolean

    fun lastModified(): Long

    fun resolve(relative: String): LocalSavePath

    fun resolveRemote(remoteName: String): LocalSavePath?

    fun walkTopDown(): Sequence<LocalSavePath>

    /** Relative names always use '/' on every host. */
    fun toRelativeString(root: LocalSavePath): String
}
