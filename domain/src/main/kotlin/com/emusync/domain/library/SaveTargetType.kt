package com.emusync.domain.library

enum class SaveTargetType {
    FILE,
    DIRECTORY,
    /** The legacy path did not exist during migration, so its type was unknowable. */
    UNKNOWN,
}
