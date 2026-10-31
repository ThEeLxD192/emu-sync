package com.emusync.config

import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetType
import com.emusync.model.AppConfig
import com.emusync.model.GoogleDriveConfig
import java.io.File
import kotlin.test.assertEquals
import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class ConfigFormatCompatibilityTest {
    @Test
    fun `legacy document preserves every entry field and serialized save type across domain conversion`(
        @TempDir directory: File
    ) {
        val base = directory.invariantSeparatorsPath
        val document =
            """
            {
              "googleDrive": {"clientId":"client","clientSecret":"secret","refreshToken":"refresh"},
              "entries": [
                {
                  "type":"emulator", "name":"System", "executablePath":"/bin/emu",
                  "arguments":["--game","{ROM}"], "romsDirectory":"/roms", "extensions":["iso","bin"],
                  "savePaths":["$base/system-file"],
                  "saveTargets":[
                    {"path":"$base/system-file","type":"file","cloudPath":"slot/save.dat"},
                    {"path":"$base/system-dir","type":"directory","cloudPath":"profile"},
                    {"path":"$base/system-unknown","type":"unknown","cloudPath":"legacy"}
                  ],
                  "savePathsByRom":{"Game.iso":["$base/game-file"]},
                  "saveTargetsByRom":{"Game.iso":[{"path":"$base/game-file","type":"file","cloudPath":"game.sav"}]},
                  "coverPathByRom":{"Game.iso":"/covers/game.png"},
                  "titleByRom":{"Game.iso":"Custom title"}, "group":"Console", "cloudFolder":"Existing saves",
                  "driveFileId":"remote-system", "steamAppId":-123, "steamProcessName":"emu-process",
                  "fullscreenArgs":"--fullscreen", "coversDirectory":"/covers"
                },
                {
                  "type":"native", "name":"Native", "executablePath":"/bin/game", "arguments":["--safe"],
                  "savePaths":["$base/native-file"],
                  "saveTargets":[{"path":"$base/native-file","type":"file","cloudPath":"native.sav"}],
                  "waitForProcess":"game-process", "group":"PC", "cloudFolder":"Native saves",
                  "driveFileId":"remote-native", "steamAppId":456, "coverPath":"/covers/native.png"
                }
              ]
            }
            """
                .trimIndent()
        val expected =
            AppConfig(
                GoogleDriveConfig("client", "secret", "refresh"),
                listOf(
                    EmulatorSystem(
                        name = "System",
                        executablePath = "/bin/emu",
                        arguments = listOf("--game", "{ROM}"),
                        romsDirectory = "/roms",
                        extensions = listOf("iso", "bin"),
                        savePaths = listOf("$base/system-file"),
                        saveTargets =
                            listOf(
                                SaveTarget(
                                    "$base/system-file",
                                    SaveTargetType.FILE,
                                    "slot/save.dat",
                                ),
                                SaveTarget("$base/system-dir", SaveTargetType.DIRECTORY, "profile"),
                                SaveTarget(
                                    "$base/system-unknown",
                                    SaveTargetType.UNKNOWN,
                                    "legacy",
                                ),
                            ),
                        savePathsByRom = mapOf("Game.iso" to listOf("$base/game-file")),
                        saveTargetsByRom =
                            mapOf(
                                "Game.iso" to
                                    listOf(
                                        SaveTarget(
                                            "$base/game-file",
                                            SaveTargetType.FILE,
                                            "game.sav",
                                        )
                                    )
                            ),
                        coverPathByRom = mapOf("Game.iso" to "/covers/game.png"),
                        titleByRom = mapOf("Game.iso" to "Custom title"),
                        group = "Console",
                        cloudFolder = "Existing saves",
                        driveFileId = "remote-system",
                        steamAppId = -123,
                        steamProcessName = "emu-process",
                        fullscreenArgs = "--fullscreen",
                        coversDirectory = "/covers",
                    ),
                    NativePCGame(
                        name = "Native",
                        executablePath = "/bin/game",
                        arguments = listOf("--safe"),
                        savePaths = listOf("$base/native-file"),
                        saveTargets =
                            listOf(
                                SaveTarget("$base/native-file", SaveTargetType.FILE, "native.sav")
                            ),
                        waitForProcess = "game-process",
                        group = "PC",
                        cloudFolder = "Native saves",
                        driveFileId = "remote-native",
                        steamAppId = 456,
                        coverPath = "/covers/native.png",
                    ),
                ),
            )
        val configFile = File(directory, "config.json").apply { writeText(document) }
        val manager = ConfigManager(configFile)

        assertEquals(expected, manager.load(autoCreate = false))
        manager.save(expected)
        assertEquals(
            Json.parseToJsonElement(document),
            Json.parseToJsonElement(configFile.readText()),
        )
    }
}
