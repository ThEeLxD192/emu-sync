package com.emusync.local.saves

import com.emusync.domain.sync.CloudSaveState
import com.emusync.domain.sync.SyncCheckpoint
import java.io.File
import kotlin.test.assertEquals
import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class SyncHistoryCompatibilityTest {
    @Test
    fun `version one history retains complete and legacy checkpoints when adding a neutral observation`(
        @TempDir directory: File
    ) {
        val existing =
            """
            {
              "formatVersion":1,
              "saves":{
                "existing":{"localSha256":"local-hash","remoteId":"remote","remoteModifiedTime":"2026-10-01T12:00:00Z","remoteSha256":"cloud-hash","remoteVersion":"42"},
                "legacy":{"localSha256":"legacy-hash","remoteId":"old-remote","remoteModifiedTime":"2026-09-01T12:00:00Z"}
              }
            }
            """
                .trimIndent()
        val history = SyncHistoryStore(File(directory, "config.json"))
        history.stateFile.writeText(existing)

        assertEquals(
            SyncCheckpoint("local-hash", "remote", "2026-10-01T12:00:00Z", "cloud-hash", "42"),
            history.get("existing"),
        )
        assertEquals(
            SyncCheckpoint("legacy-hash", "old-remote", "2026-09-01T12:00:00Z"),
            history.get("legacy"),
        )
        assertEquals(existing, history.stateFile.readText())

        history.record(
            "new",
            "new-local",
            CloudSaveState("new-remote", "2026-10-30T12:00:00Z", "new-cloud", "7"),
        )

        // The existing writer omits the default formatVersion; keys and metadata stay intact.
        val expected =
            """
            {
              "saves":{
                "existing":{"localSha256":"local-hash","remoteId":"remote","remoteModifiedTime":"2026-10-01T12:00:00Z","remoteSha256":"cloud-hash","remoteVersion":"42"},
                "legacy":{"localSha256":"legacy-hash","remoteId":"old-remote","remoteModifiedTime":"2026-09-01T12:00:00Z"},
                "new":{"localSha256":"new-local","remoteId":"new-remote","remoteModifiedTime":"2026-10-30T12:00:00Z","remoteSha256":"new-cloud","remoteVersion":"7"}
              }
            }
            """
                .trimIndent()
        assertEquals(
            Json.parseToJsonElement(expected),
            Json.parseToJsonElement(history.stateFile.readText()),
        )
    }
}
