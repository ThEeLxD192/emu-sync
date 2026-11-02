package com.emusync.domain.sync

import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetType
import kotlin.test.assertEquals
import org.junit.jupiter.api.Test

class CloudPathsTest {
    @Test
    fun `getCloudPathSegments respects game title for EmulatorSystem with savePathsByRom`() {

        // Case: Eden emulator grouped under "Switch", individual save paths for Crash Team Racing
        val system =
            EmulatorSystem(
                name = "Eden",
                executablePath = "/usr/bin/eden",
                romsDirectory = "/roms/switch",
                extensions = listOf("nsp", "xci"),
                group = "Switch",
                savePaths = emptyList(),
                savePathsByRom =
                    mapOf(
                        "Crash Team Racing Nitro-Fueled.nsp" to
                            listOf("/saves/OCTANE_0.sav", "/saves/OCTANE_OPTIONS.sav")
                    ),
            )

        val pathSegments =
            pathSegments(
                entry = system,
                romName = "Crash Team Racing Nitro-Fueled.nsp",
                gameTitle = "Crash Team Racing Nitro-Fueled",
            )

        assertEquals(listOf("Switch", "Crash Team Racing Nitro-Fueled"), pathSegments)
    }

    @Test
    fun `global save paths use emulator cloud folder despite empty or copied rom overrides`() {
        val baseSystem =
            EmulatorSystem(
                name = "RPCS3",
                executablePath = "/usr/bin/rpcs3",
                romsDirectory = "/games/ps3",
                extensions = listOf("iso"),
                group = "PlayStation",
                savePaths = listOf("/mnt/Juegos/Saves/PS3"),
            )

        val emptyOverride = baseSystem.copy(savePathsByRom = mapOf("Black.iso" to emptyList()))
        val copiedGlobalOverride =
            baseSystem.copy(savePathsByRom = mapOf("Black.iso" to listOf("/mnt/Juegos/Saves/PS3")))
        val distinctPerGameOverride =
            baseSystem.copy(
                savePathsByRom = mapOf("Black.iso" to listOf("/mnt/Juegos/Saves/PS3/Black"))
            )

        assertEquals(
            listOf("PlayStation"),
            pathSegments(baseSystem, "Black.iso", "Black"),
        )
        assertEquals(
            listOf("PlayStation"),
            pathSegments(emptyOverride, "Black.iso", "Black"),
        )
        assertEquals(
            listOf("PlayStation"),
            pathSegments(copiedGlobalOverride, "Black.iso", "Black"),
        )
        assertEquals(
            listOf("PlayStation", "Black"),
            pathSegments(distinctPerGameOverride, "Black.iso", "Black"),
        )
    }

    @Test
    fun `getCloudPathSegments handles custom title in titleByRom`() {

        val system =
            EmulatorSystem(
                name = "Eden",
                executablePath = "/usr/bin/eden",
                romsDirectory = "/roms/switch",
                extensions = listOf("nsp", "xci"),
                group = "Switch",
                savePaths = emptyList(),
                savePathsByRom = mapOf("CTR_NF.nsp" to listOf("/saves/OCTANE_0.sav")),
                titleByRom = mapOf("CTR_NF.nsp" to "Crash Team Racing Nitro-Fueled"),
            )

        val pathSegments =
            pathSegments(
                entry = system,
                romName = "CTR_NF.nsp",
            )

        assertEquals(listOf("Switch", "Crash Team Racing Nitro-Fueled"), pathSegments)
    }

    @Test
    fun `getCloudPathSegments for NativePCGame in a group uses group and game name`() {

        val gameInGroup =
            NativePCGame(
                name = "Spelunky Classic",
                executablePath = "/games/spelunky",
                group = "Indie",
                savePaths = listOf("/saves/spelunky.dat"),
            )
        assertEquals(
            listOf("Indie", "Spelunky Classic"),
            pathSegments(gameInGroup),
        )

        val gameWithoutGroup =
            NativePCGame(
                name = "Cave Story",
                executablePath = "/games/doukutsu",
                group = null,
                savePaths = listOf("/saves/Profile.dat"),
            )
        assertEquals(listOf("Cave Story"), pathSegments(gameWithoutGroup))
    }

    private fun pathSegments(
        entry: GameEntry,
        romName: String? = null,
        gameTitle: String? = null,
    ): List<String> {
        val global = entry.savePaths.map { SaveTarget(it, SaveTargetType.UNKNOWN) }
        val perRom =
            (entry as? EmulatorSystem)?.savePathsByRom?.get(romName).orEmpty().map {
                SaveTarget(it, SaveTargetType.UNKNOWN)
            }
        return cloudPathSegments(entry, romName, gameTitle, global, perRom)
    }
}
