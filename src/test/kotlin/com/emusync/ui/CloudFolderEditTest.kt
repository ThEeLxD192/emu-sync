package com.emusync.ui

import com.emusync.config.ConfigManager
import com.emusync.drive.MigrationDrive
import com.emusync.model.AppConfig
import com.emusync.model.EmulatorSystem
import com.emusync.model.GameEntry
import com.emusync.model.GoogleDriveConfig
import com.emusync.model.NativePCGame
import com.emusync.model.effectiveCloudFolder
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class CloudFolderEditTest {
    @Test
    fun `entry migration preserves unrelated configuration changes while waiting for Drive`(@TempDir directory: File) = runTest {
        MigrationDrive().use { drive ->
            drive.folder("source", "Old")
            drive.folder("destination", "New")
            drive.file("save", "save.dat", "source")
            val entry = NativePCGame("Old", "echo")
            val (viewModel, manager) = viewModel(drive, directory, listOf(entry))
            val started = CompletableDeferred<Unit>()
            val resumeMove = CompletableDeferred<Unit>()
            drive.onMove = {
                started.complete(Unit)
                resumeMove.await()
            }

            val migration = async { viewModel.editEntry(entry, entry.copy(name = "New")) }
            started.await()
            val added = NativePCGame("Added during migration", "echo")
            viewModel.addEntry(added)
            resumeMove.complete(Unit)

            assertTrue(migration.await())
            assertEquals(listOf("New", added.name), manager.load().entries.map { it.name })
            assertEquals(manager.load(), viewModel.uiState.value.config)
        }
    }

    @Test
    fun `regrouping a shared emulator keeps its cloud location and other entries`(@TempDir directory: File) = runTest {
        MigrationDrive().use { drive ->
            val first = emulator(directory, "First").copy(group = "Switch")
            val second = emulator(directory, "Second").copy(group = "Switch")
            val (viewModel, manager) = viewModel(drive, directory, listOf(first, second))

            assertTrue(viewModel.editEntry(first, first.copy(group = "Other Group")))

            val edited = viewModel.uiState.value.config!!.entries.first()
            assertEquals("Other Group", edited.group)
            assertEquals("Switch", edited.cloudFolder)
            assertEquals("Switch", edited.effectiveCloudFolder)
            assertEquals(second, viewModel.uiState.value.config!!.entries.last())
            assertEquals(viewModel.uiState.value.config, manager.load())
            assertTrue(drive.mutations.isEmpty())
        }
    }

    @Test
    fun `explicit migration of a shared root is rejected before saving`(@TempDir directory: File) = runTest {
        MigrationDrive().use { drive ->
            val first = emulator(directory, "First").copy(group = "Switch")
            val second = emulator(directory, "Second").copy(group = "Switch")
            val (viewModel, manager) = viewModel(drive, directory, listOf(first, second))
            val originalConfig = manager.load()

            assertFalse(viewModel.editEntry(first, first.copy(cloudFolder = "New Cloud Folder")))

            assertEquals(originalConfig, manager.load())
            assertEquals(originalConfig, viewModel.uiState.value.config)
            assertTrue((viewModel.uiState.value.status as AppStatus.Error).message.contains("shared"))
            assertTrue(drive.mutations.isEmpty())
        }
    }

    @Test
    fun `blocked entry merge keeps old configuration and both remote copies`(@TempDir directory: File) = runTest {
        MigrationDrive().use { drive ->
            drive.folder("source", "Old")
            drive.folder("destination", "New")
            drive.file("old-save", "save.dat", "source")
            drive.file("new-save", "save.dat", "destination")
            val entry = NativePCGame("Old", "echo")
            val (viewModel, manager) = viewModel(drive, directory, listOf(entry))
            val originalConfig = manager.load()

            assertFalse(viewModel.editEntry(entry, entry.copy(name = "New")))

            assertEquals(originalConfig, manager.load())
            assertEquals(originalConfig, viewModel.uiState.value.config)
            assertTrue(viewModel.uiState.value.status is AppStatus.Error)
            assertTrue(drive.mutations.isEmpty())
        }
    }

    @Test
    fun `blocked game title merge keeps its previous title and remote saves`(@TempDir directory: File) = runTest {
        MigrationDrive().use { drive ->
            drive.folder("console", "Console")
            drive.folder("source", "Old", "console")
            drive.folder("destination", "New", "console")
            drive.file("old-save", "save.dat", "source")
            drive.file("new-save", "save.dat", "destination")
            val entry = emulator(directory, "Console").copy(titleByRom = mapOf("Game.rom" to "Old"))
            val rom = File(directory, "Game.rom").apply { writeText("ROM") }
            val (viewModel, manager) = viewModel(drive, directory, listOf(entry))
            val originalConfig = manager.load()

            viewModel.editGameOverride(GameItem("Game", entry, rom), emptyList(), newTitle = "New")

            assertEquals(originalConfig, manager.load())
            assertEquals(originalConfig, viewModel.uiState.value.config)
            assertTrue(viewModel.uiState.value.status is AppStatus.Error)
            assertTrue(drive.mutations.isEmpty())
        }
    }

    @Test
    fun `renaming grouped native game updates only its own folder once`(@TempDir directory: File) = runTest {
        MigrationDrive().use { drive ->
            drive.folder("group", "Ports")
            drive.folder("game", "Old", "group")
            drive.folder("other", "Other", "group")
            drive.file("save", "save.dat", "game")
            val entry = NativePCGame("Old", "echo", group = "Ports")
            val other = NativePCGame("Other", "echo", group = "Ports")
            val (viewModel, manager) = viewModel(drive, directory, listOf(entry, other))

            viewModel.editGameOverride(GameItem("Old", entry), emptyList(), newTitle = "New")

            assertEquals("New", drive.nodes.getValue("game").name)
            assertEquals("group", drive.nodes.getValue("game").parent)
            assertEquals("group", drive.nodes.getValue("other").parent)
            assertEquals(listOf("PATCH:game"), drive.mutations)
            assertEquals("New", manager.load().entries.first().name)
            assertEquals(other, manager.load().entries.last())
        }
    }

    @Test
    fun `clearing a custom title migrates saves back to the default title`(@TempDir directory: File) = runTest {
        MigrationDrive().use { drive ->
            drive.folder("console", "Console")
            drive.folder("game", "Custom Title", "console")
            drive.file("save", "save.dat", "game")
            val entry = emulator(directory, "Console").copy(titleByRom = mapOf("Game.rom" to "Custom Title"))
            val rom = File(directory, "Game.rom").apply { writeText("ROM") }
            val (viewModel, manager) = viewModel(drive, directory, listOf(entry))

            viewModel.editGameOverride(GameItem("Game", entry, rom), emptyList(), newTitle = "")

            assertEquals("Game", drive.nodes.getValue("game").name)
            assertTrue((manager.load().entries.single() as EmulatorSystem).titleByRom.isEmpty())
            assertEquals("game", drive.nodes.getValue("save").parent)
        }
    }

    private fun emulator(directory: File, name: String) = EmulatorSystem(
        name, "echo", romsDirectory = directory.path, extensions = listOf("rom"),
    )

    private suspend fun viewModel(drive: MigrationDrive, directory: File, entries: List<GameEntry>): Pair<AppViewModel, ConfigManager> {
        val manager = ConfigManager(File(directory, "config.json"))
        manager.save(AppConfig(GoogleDriveConfig("client", "secret", "refresh"), entries))
        val viewModel = AppViewModel(configManager = manager, httpClient = drive.client)
        viewModel.loadConfig()
        return viewModel to manager
    }
}
