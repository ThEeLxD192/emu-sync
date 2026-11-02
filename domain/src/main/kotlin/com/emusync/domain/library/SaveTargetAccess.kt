package com.emusync.domain.library

/** Resolves persisted and restored save targets against the local filesystem. */
interface SaveTargetAccess {
    /** Returns the physical type, or null for a blank or absent path. */
    fun typeOf(path: String): SaveTargetType?

    fun forEntry(entry: GameEntry): List<SaveTarget>

    fun forRom(entry: EmulatorSystem, romName: String): List<SaveTarget>

    fun forRestore(destination: String, cloudNames: List<String>): List<SaveTarget>
}
