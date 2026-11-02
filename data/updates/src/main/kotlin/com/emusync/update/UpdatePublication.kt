package com.emusync.update

import java.nio.file.AtomicMoveNotSupportedException
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.StandardCopyOption

internal fun moveReplacing(source: Path, target: Path) {
    try {
        Files.move(
            source,
            target,
            StandardCopyOption.ATOMIC_MOVE,
            StandardCopyOption.REPLACE_EXISTING,
        )
    } catch (_: AtomicMoveNotSupportedException) {
        Files.move(source, target, StandardCopyOption.REPLACE_EXISTING)
    }
}
