package com.emusync.application.library

import com.emusync.application.operation.OperationCoordinator
import com.emusync.domain.config.AppConfig
import com.emusync.domain.library.GameEntry
import com.emusync.domain.sync.SaveSynchronizerFactory
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong

/** Rejects observations from older checks, connection changes, edits or save mutations. */
class EntrySyncChecks(
    private val operations: OperationCoordinator,
    private val synchronizers: SaveSynchronizerFactory,
) {
    private val generation = AtomicLong()
    private val checks = ConcurrentHashMap<String, Long>()

    fun begin(entry: GameEntry, config: AppConfig): SyncCheckRequest {
        val request = generation.incrementAndGet()
        checks[entry.name] = request
        return SyncCheckRequest(entry, config, request, operations.revision)
    }

    fun isCurrent(request: SyncCheckRequest, current: AppConfig?): Boolean =
        checks[request.entry.name] == request.generation &&
            operations.revision == request.operationRevision &&
            !operations.isBusy &&
            current?.cloudCredentials == request.config.cloudCredentials &&
            current?.entries?.any { it == request.entry } == true

    suspend fun read(request: SyncCheckRequest) =
        synchronizers.create(request.config) {}.checkEntrySyncStatus(request.entry)
}
