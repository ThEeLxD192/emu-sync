package com.emusync.domain.library

/** Gives newly configured directory roots distinct, portable cloud paths. */
fun scopeNewSaveTargets(targets: List<SaveTarget>): List<SaveTarget> {
    if (targets.count { it.type == SaveTargetType.DIRECTORY } < 2) return targets
    val used = targets.mapNotNull { it.cloudPath }.toMutableSet()
    var next = 1
    return targets.map { target ->
        if (target.type != SaveTargetType.DIRECTORY || !target.cloudPath.isNullOrBlank()) target
        else {
            var prefix: String
            do {
                prefix = "save-target-${next++}"
            } while (
                used.any { it == prefix || it.startsWith("$prefix/") || prefix.startsWith("$it/") }
            )
            used += prefix
            target.copy(cloudPath = prefix)
        }
    }
}
