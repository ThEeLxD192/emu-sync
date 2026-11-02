package com.emusync.application.sync

import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetType
import kotlin.test.assertEquals
import org.junit.jupiter.api.Test

class SaveTargetResolverTest {
    @Test
    fun `ROM mapping with a different cloud prefix is distinct from the global fallback`() {
        val path = "same-local-root"
        val system =
            EmulatorSystem(
                "Console",
                "echo",
                romsDirectory = "roms",
                extensions = listOf("rom"),
                saveTargets = listOf(SaveTarget(path, SaveTargetType.DIRECTORY, "global")),
                saveTargetsByRom =
                    mapOf(
                        "Game.rom" to
                            listOf(SaveTarget(path, SaveTargetType.DIRECTORY, "game-specific"))
                    ),
            )
        assertEquals(
            listOf("Console", "Game"),
            SaveTargetResolver(MemorySaveAccess()).getCloudPathSegments(system, "Game.rom"),
        )
        assertEquals(
            setOf("global", "game-specific"),
            SaveTargetResolver(MemorySaveAccess())
                .getSaveTargets(system)
                .map { it.cloudPath }
                .toSet(),
        )
    }
}
