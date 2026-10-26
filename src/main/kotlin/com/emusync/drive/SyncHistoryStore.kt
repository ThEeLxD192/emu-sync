package com.emusync.drive

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.io.File
import java.io.IOException
import java.nio.channels.FileChannel
import java.nio.file.AtomicMoveNotSupportedException
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.StandardCopyOption
import java.nio.file.StandardOpenOption
import java.nio.file.attribute.PosixFilePermissions
import java.security.MessageDigest
import java.util.concurrent.ConcurrentHashMap

internal class SaveSyncException(message: String, cause: Throwable? = null) : Exception(message, cause)

@Serializable
internal data class SyncedSave(
    val localSha256: String,
    val remoteId: String,
    val remoteModifiedTime: String,
    val remoteSha256: String? = null,
    val remoteVersion: String? = null,
)

@Serializable
private data class SyncHistory(val formatVersion: Int = 1, val saves: Map<String, SyncedSave> = emptyMap())

/** Local-only synchronization evidence. Credentials and save contents never enter the index. */
internal class SyncHistoryStore(
    configFile: File,
    private val beforeReplace: (Path) -> Unit = {},
) {
    val stateFile = File(configFile.absoluteFile.parentFile, "${configFile.name}.sync-state.json")
    val backupDirectory = File(configFile.absoluteFile.parentFile, "${configFile.name}.sync-backups")
    private val lock = locks.computeIfAbsent(stateFile.toPath().normalize().toString()) { Any() }
    private val json = Json { prettyPrint = true; ignoreUnknownKeys = true }

    fun key(file: File, folderId: String, remoteName: String): String = sha256(
        "${file.toPath().toAbsolutePath().normalize()}\u0000$folderId\u0000$remoteName".toByteArray(),
    )

    fun get(key: String): SyncedSave? = synchronized(lock) { read().saves[key] }

    fun record(key: String, digest: String, remote: DriveFile) = synchronized(lock) {
        val state = read()
        val checkpoint = SyncedSave(digest, remote.id, remote.modifiedTime, remote.sha256Checksum, remote.version)
        if (state.saves[key] != checkpoint) {
            writeAtomically(stateFile, json.encodeToString(SyncHistory.serializer(), state.copy(saves = state.saves + (key to checkpoint))))
        }
    }

    fun backup(key: String, file: File, digest: String) = synchronized(lock) {
        val location = File(backupDirectory, key)
        val versionDirectory = File(location, digest)
        try {
            Files.createDirectories(versionDirectory.toPath())
            restrict(versionDirectory.toPath(), "rwx------")
            val destination = File(versionDirectory, file.name)
            if (destination.exists()) {
                if (sha256(destination) != digest) throw IOException("An existing recovery copy was modified")
            } else {
                val temporary = Files.createTempFile(versionDirectory.toPath(), ".save.", ".tmp")
                try {
                    Files.copy(file.toPath(), temporary, StandardCopyOption.REPLACE_EXISTING)
                    restrict(temporary, "rw-------")
                    if (sha256(temporary.toFile()) != digest) throw IOException("The local save changed while creating its backup")
                    moveReplacing(temporary, destination.toPath())
                } finally { Files.deleteIfExists(temporary) }
            }
            writeAtomically(File(location, "source.json"), json.encodeToString(
                kotlinx.serialization.serializer<Map<String, String>>(), mapOf("originalPath" to file.absolutePath),
            ))
        } catch (failure: Exception) {
            throw SaveSyncException("Could not preserve a recovery copy of ${file.name}. The local save was not replaced.", failure)
        }
    }

    private fun read(): SyncHistory {
        if (!stateFile.exists()) return SyncHistory()
        try {
            val state = json.decodeFromString<SyncHistory>(stateFile.readText())
            if (state.formatVersion != 1) throw IOException("Unsupported synchronization history format")
            return state
        } catch (failure: Exception) {
            throw SaveSyncException("Could not read synchronization history at ${stateFile.absolutePath}. No save was overwritten.", failure)
        }
    }

    private fun writeAtomically(file: File, contents: String) {
        var temporary: Path? = null
        try {
            Files.createDirectories(file.absoluteFile.parentFile.toPath())
            temporary = Files.createTempFile(file.absoluteFile.parentFile.toPath(), ".${file.name}.", ".tmp")
            Files.writeString(temporary, contents)
            restrict(temporary, "rw-------")
            FileChannel.open(temporary, StandardOpenOption.WRITE).use { it.force(true) }
            beforeReplace(temporary)
            moveReplacing(temporary, file.toPath())
            temporary = null
        } catch (failure: Exception) {
            throw SaveSyncException("Could not persist synchronization history at ${file.absolutePath}.", failure)
        } finally { temporary?.let { Files.deleteIfExists(it) } }
    }

    companion object {
        private val locks = ConcurrentHashMap<String, Any>()
    }
}

internal fun sha256(file: File): String {
    val digest = MessageDigest.getInstance("SHA-256")
    file.inputStream().buffered().use { input ->
        val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
        while (true) {
            val count = input.read(buffer)
            if (count < 0) break
            digest.update(buffer, 0, count)
        }
    }
    return digest.digest().joinToString("") { "%02x".format(it) }
}

internal fun sha256(bytes: ByteArray): String = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

internal fun moveReplacing(source: Path, target: Path) {
    try {
        Files.move(source, target, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
    } catch (_: AtomicMoveNotSupportedException) {
        Files.move(source, target, StandardCopyOption.REPLACE_EXISTING)
    }
}

private fun restrict(path: Path, permissions: String) {
    try { Files.setPosixFilePermissions(path, PosixFilePermissions.fromString(permissions)) }
    catch (_: UnsupportedOperationException) { /* Windows uses the user's directory ACLs. */ }
}
