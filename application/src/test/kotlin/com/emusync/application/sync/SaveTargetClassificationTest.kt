package com.emusync.application.sync

import com.emusync.domain.library.SaveTargetType
import com.emusync.domain.sync.CloudFile
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import org.junit.jupiter.api.Test

class SaveTargetClassificationTest {
    @Test
    fun `unknown missing target ignores exact cloud name owned by sibling target`() {
        val files = MemorySaveAccess()
        val missingTarget = files.path("SAVE_SLOT")
        val mapping = SaveTargetMapping(files)
        val cloudSave = CloudFile("save-id", "SAVE_SLOT", "2026-09-25T18:00:00.000Z")

        assertFalse(
            mapping.isDirectorySaveTarget(SaveTargetType.UNKNOWN, missingTarget, listOf(cloudSave)),
            "A matching Drive filename identifies a missing legacy target as a file",
        )
        assertTrue(
            mapping.isDirectorySaveTarget(
                SaveTargetType.UNKNOWN,
                missingTarget,
                listOf(cloudSave),
                setOf("SAVE_SLOT"),
            ),
            "A matching cloud file claimed by a known file target must not classify this target as a file",
        )
        assertFalse(
            mapping.isDirectorySaveTarget(SaveTargetType.UNKNOWN, missingTarget, listOf(cloudSave)),
            "An UNKNOWN sibling with the same name cannot claim the exact cloud match",
        )
        assertTrue(
            mapping.isDirectorySaveTarget(
                SaveTargetType.UNKNOWN,
                missingTarget,
                listOf(cloudSave.copy(name = "slot/save.dat")),
            ),
            "Nested cloud names identify a missing legacy target as a directory",
        )
    }
}
