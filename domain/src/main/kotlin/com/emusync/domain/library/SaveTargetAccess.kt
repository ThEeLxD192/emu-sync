package com.emusync.domain.library

/** Resolves persisted and restored save targets against the local filesystem. */
interface SaveTargetAccess {
    fun forEntry(entry: GameEntry): List<SaveTarget>

    fun forRom(entry: EmulatorSystem, romName: String): List<SaveTarget>

    fun forRestore(destination: String, cloudNames: List<String>): List<SaveTarget>
}
