package com.emusync.local.saves

import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.sync.LocalSaveAccess
import com.emusync.domain.sync.LocalSavePath
import java.io.File

class DesktopSaveAccess : LocalSaveAccess {
    override fun path(path: String): LocalSavePath = DesktopSavePath(File(path))

    override fun effectiveSaveTargets(entry: GameEntry) = entry.effectiveSaveTargets()

    override fun effectiveSaveTargetsForRom(entry: EmulatorSystem, romName: String) =
        entry.effectiveSaveTargetsForRom(romName)
}

private class DesktopSavePath(private val file: File) : LocalSavePath {
    override val path: String
        get() = file.path

    override val name: String
        get() = file.name

    override val absolutePath: String
        get() = file.absolutePath

    override val normalizedPath: String
        get() = file.absoluteFile.normalize().path

    override val ancestorPaths: Sequence<String>
        get() =
            generateSequence(file.absoluteFile.normalize().parentFile) { it.parentFile }
                .map { it.path }

    override val isFile
        get() = file.isFile

    override val isDirectory
        get() = file.isDirectory

    override fun exists() = file.exists()

    override fun lastModified() = file.lastModified()

    override fun resolve(relative: String) = DesktopSavePath(file.resolve(relative))

    override fun resolveRemote(remoteName: String) =
        resolveRemoteFile(file, remoteName)?.let(::DesktopSavePath)

    override fun walkTopDown() = file.walkTopDown().map(::DesktopSavePath)

    override fun toRelativeString(root: LocalSavePath) =
        file.toRelativeString(File(root.path)).replace(File.separatorChar, '/')
}
