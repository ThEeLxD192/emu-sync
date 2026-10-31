package com.emusync.application.sync

import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetType
import com.emusync.domain.sync.LocalSaveAccess
import com.emusync.domain.sync.LocalSavePath
import java.nio.file.Path

/** Local observations without disk access; these scenarios use absent, explicitly typed paths. */
internal class MemorySaveAccess : LocalSaveAccess {
    override fun path(path: String): LocalSavePath = MissingSavePath(Path.of(path))

    override fun effectiveSaveTargets(entry: GameEntry) =
        entry.saveTargets.ifEmpty { entry.savePaths.map { SaveTarget(it, SaveTargetType.UNKNOWN) } }

    override fun effectiveSaveTargetsForRom(entry: EmulatorSystem, romName: String) =
        entry.saveTargetsByRom[romName].orEmpty()
}

private class MissingSavePath(private val value: Path) : LocalSavePath {
    override val path
        get() = value.toString()

    override val name
        get() = value.fileName.toString()

    override val absolutePath
        get() = value.toAbsolutePath().toString()

    override val normalizedPath: String
        get() = value.toAbsolutePath().normalize().toString()

    override val ancestorPaths: Sequence<String>
        get() =
            generateSequence(value.toAbsolutePath().normalize().parent) { it.parent }
                .map { it.toString() }

    override val isFile = false
    override val isDirectory = false

    override fun exists() = false

    override fun lastModified() = 0L

    override fun resolve(relative: String) = MissingSavePath(value.resolve(relative))

    override fun resolveRemote(remoteName: String) = MissingSavePath(value.resolve(remoteName))

    override fun walkTopDown() = sequenceOf(this)

    override fun toRelativeString(root: LocalSavePath) =
        Path.of(root.path).relativize(value).toString().replace('\\', '/')
}
