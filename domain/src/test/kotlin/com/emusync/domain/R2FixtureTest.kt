package com.emusync.domain

import com.emusync.domain.execution.ArgumentsCodec
import com.emusync.domain.library.cleanGameTitle
import com.emusync.domain.sync.CloudFileState
import com.emusync.domain.sync.CloudSaveState
import com.emusync.domain.sync.LocalFileState
import com.emusync.domain.sync.LocalNewerAction
import com.emusync.domain.sync.SyncCheckpoint
import com.emusync.domain.sync.planDirectorySync
import com.emusync.domain.sync.planSyncDecision
import com.emusync.domain.sync.planTrackedSync
import com.emusync.domain.sync.remotePathSegments
import java.time.Instant
import kotlin.test.Test
import kotlin.test.assertEquals

class R2FixtureTest {
    @Test
    fun `shared migration fixtures retain reference behavior`() {
        val lines = checkNotNull(javaClass.getResourceAsStream("/r2/comparison.tsv"))
        lines.bufferedReader().useLines { fixtures ->
            fixtures
                .filterNot { it.startsWith("#") || it.isBlank() }
                .forEach { line ->
                    val fields = line.split('\t')
                    val actual =
                        when (fields[0]) {
                            "conflict" -> conflict(fields)
                            "tracked" -> tracked(fields)
                            "directory" -> {
                                val local =
                                    fields[1].split(';').map {
                                        val parts = it.split('@')
                                        LocalFileState(parts[0], parts[1].toLong())
                                    }
                                val cloud =
                                    fields[2].split(';').map {
                                        val parts = it.split('@')
                                        CloudFileState(
                                            parts[0],
                                            parts[1],
                                            Instant.ofEpochMilli(parts[2].toLong()).toString(),
                                        )
                                    }
                                planDirectorySync(local, cloud).joinToString(";") {
                                    "${it.relativePath}:${it.decision.name}:${it.cloud?.id ?: "-"}:${it.duplicateCloudFiles.joinToString(",") { file -> file.id }}"
                                }
                            }
                            "remote" ->
                                remotePathSegments(fields[1])?.joinToString("|") ?: "INVALID"
                            "arguments" ->
                                ArgumentsCodec.parse(fields[1])?.joinToString("|") ?: "INVALID"
                            "title" -> cleanGameTitle(fields[1])
                            else -> error("Unknown fixture: $line")
                        }
                    assertEquals(fields.last(), actual, line)
                }
        }
    }

    private fun conflict(fields: List<String>): String {
        val local = fields[1].toLongOrNull()
        val cloud = fields[2].toLongOrNull()?.let { Instant.ofEpochMilli(it).toString() }
        val action =
            if (fields[3] == "UPLOAD") LocalNewerAction.UPLOAD
            else LocalNewerAction.RESOLVE_CONFLICT
        return planSyncDecision(local, cloud, localNewerAction = action).name
    }

    private fun tracked(fields: List<String>): String {
        val hash = fields[2].takeUnless { it == "null" }
        val remote = CloudSaveState("id", "2026-01-01T00:00:00Z", hash, fields[3])
        val previous =
            if (fields[4] == "yes") SyncCheckpoint("old", "id", "2026-01-01T00:00:00Z", "old", "1")
            else null
        return planTrackedSync(fields[1], remote, previous).name
    }
}
