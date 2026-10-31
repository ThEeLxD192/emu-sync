import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.painter.BitmapPainter
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.graphics.toComposeImageBitmap
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Window
import androidx.compose.ui.window.WindowPosition
import androidx.compose.ui.window.application
import androidx.compose.ui.window.rememberWindowState
import com.emusync.config.ConfigManager
import com.emusync.domain.library.GameEntry
import com.emusync.ui.AppViewModel
import com.emusync.ui.CloudSyncStatus
import com.emusync.ui.GameItem
import com.emusync.ui.components.AppHeader
import com.emusync.ui.components.GameGrid
import com.emusync.ui.components.Sidebar
import com.emusync.ui.dialogs.AddEntryDialog
import com.emusync.ui.dialogs.DriveSetupDialog
import com.emusync.ui.dialogs.EditGameDialog
import com.emusync.ui.dialogs.StatusOverlay
import com.emusync.ui.dialogs.UpdateDialog
import com.emusync.ui.theme.EmuSyncColors
import com.emusync.ui.theme.EmuSyncDarkScheme
import javax.swing.UIManager
import kotlinx.coroutines.launch
import org.jetbrains.skia.Image

fun main() {
    try {
        UIManager.setLookAndFeel(UIManager.getSystemLookAndFeelClassName())
    } catch (_: Exception) {
        // Fallback to default if system L&F fails
    }

    application {
        val windowState =
            rememberWindowState(
                width = 1280.dp,
                height = 800.dp,
                position = WindowPosition(Alignment.Center),
            )

        val appIcon = remember { loadAppIcon() }

        Window(
            onCloseRequest = ::exitApplication,
            title = "EmuSync",
            icon = appIcon,
            state = windowState,
        ) {
            val viewModel = remember { AppViewModel(ConfigManager()) }
            DisposableEffect(viewModel) { onDispose { viewModel.close() } }

            // Load config on first composition
            LaunchedEffect(Unit) {
                viewModel.loadConfig()
            }

            EmuSyncApp(viewModel, onExit = ::exitApplication)
        }
    }
}

@Composable
fun EmuSyncApp(viewModel: AppViewModel, onExit: () -> Unit) {
    val uiState by viewModel.uiState.collectAsState()
    val scope = rememberCoroutineScope()
    var showAddDialog by remember { mutableStateOf(false) }
    var showDriveSetupDialog by remember { mutableStateOf(false) }
    var entryToEdit by remember { mutableStateOf<GameEntry?>(null) }
    var gameToEdit by remember { mutableStateOf<GameItem?>(null) }
    var steamMessage by remember { mutableStateOf<String?>(null) }

    LaunchedEffect(Unit) {
        viewModel.checkForUpdates(manual = false)
    }

    MaterialTheme(colorScheme = EmuSyncDarkScheme) {
        Surface(
            modifier = Modifier.fillMaxSize(),
            color = EmuSyncColors.Background,
        ) {
            Column(modifier = Modifier.fillMaxSize()) {
                AppHeader(
                    state = uiState,
                    onShowDriveSetup = { showDriveSetupDialog = true },
                    onConnectDrive = {
                        scope.launch { steamMessage = viewModel.loginGoogleDrive() }
                    },
                    onShowUpdateDialog = { viewModel.showUpdateDialog() },
                    onExit = onExit,
                )

                HorizontalDivider(
                    color = EmuSyncColors.Divider,
                    thickness = 1.dp,
                )

                // ── Main Content: Sidebar + Game Grid ───────────────
                Row(modifier = Modifier.fillMaxSize()) {
                    // Left sidebar (categories)
                    Sidebar(
                        entries = uiState.config?.entries.orEmpty(),
                        selectedEntry = uiState.selectedEntry,
                        onEntrySelected = { entry ->
                            scope.launch { viewModel.selectEntry(entry) }
                        },
                        onAddClicked = { showAddDialog = true },
                        onReorder = { fromIndex, toIndex ->
                            scope.launch { viewModel.reorderEntries(fromIndex, toIndex) }
                        },
                        onReorderEntries = { newEntries ->
                            scope.launch { viewModel.reorderEntriesList(newEntries) }
                        },
                        steamAvailable = viewModel.isSteamAvailable(),
                        isSteamRegistered = { entry -> viewModel.isSteamRegistered(entry) },
                        onSteamToggle = { entry ->
                            scope.launch {
                                val isRegistered = viewModel.isSteamRegistered(entry)
                                if (isRegistered) {
                                    viewModel.unregisterFromSteam(entry)
                                    steamMessage = "\"${entry.name}\" removed from Steam."
                                } else {
                                    val result = viewModel.registerInSteam(entry)
                                    steamMessage = result
                                }
                            }
                        },
                        modifier = Modifier.width(240.dp).fillMaxHeight(),
                    )

                    VerticalDivider(
                        color = EmuSyncColors.Divider,
                        modifier = Modifier.fillMaxHeight(),
                    )

                    // Right content area (game grid)
                    GameGrid(
                        items = uiState.gameItems,
                        selectedEntry = uiState.selectedEntry,
                        isLoading = uiState.isLoading,
                        syncStatus =
                            uiState.entrySyncStatus[uiState.selectedEntry?.name]
                                ?: CloudSyncStatus.IDLE,
                        onGameClicked = { gameItem ->
                            scope.launch { viewModel.launchGame(gameItem) }
                        },
                        onOpenEmulator = { entry ->
                            scope.launch { viewModel.openEmulator(entry) }
                        },
                        onSyncCategory = {
                            uiState.selectedEntry?.let { entry ->
                                scope.launch { viewModel.syncEntryNow(entry) }
                            }
                        },
                        onCheckSync = {
                            uiState.selectedEntry?.let { entry ->
                                scope.launch { viewModel.checkSyncForEntry(entry) }
                            }
                        },
                        onEditGame = { gameItem ->
                            gameToEdit = gameItem
                        },
                        onEditCategory = {
                            entryToEdit = uiState.selectedEntry
                            showAddDialog = true
                        },
                        onDeleteCategory = {
                            uiState.selectedEntry?.let { entry ->
                                scope.launch { viewModel.deleteEntry(entry) }
                            }
                        },
                        modifier = Modifier.weight(1f).fillMaxHeight().padding(20.dp),
                    )
                }
            }

            // ── Status Overlays (on top of everything) ──────────────
            StatusOverlay(
                status = uiState.status,
                onDismissError = { viewModel.clearStatus() },
            )

            // ── Add Entry Dialog ────────────────────────────────────
            if (showAddDialog) {
                val existingGroups =
                    uiState.config
                        ?.entries
                        ?.mapNotNull { it.group?.trim()?.takeIf { g -> g.isNotBlank() } }
                        ?.distinct()
                        .orEmpty()
                AddEntryDialog(
                    initialEntry = entryToEdit,
                    existingGroups = existingGroups,
                    onDismiss = {
                        showAddDialog = false
                        entryToEdit = null
                    },
                    onSave = { newEntry ->
                        val originalEntry = entryToEdit
                        showAddDialog = false
                        scope.launch {
                            if (originalEntry == null) {
                                viewModel.addEntry(newEntry)
                            } else {
                                viewModel.editEntry(originalEntry, newEntry)
                            }
                            entryToEdit = null
                        }
                    },
                )
            }

            // ── Edit Game Save Dialog (Manual Edit) ─────────────────
            gameToEdit?.let { gameItem ->
                EditGameDialog(
                    game = gameItem,
                    onDismiss = { gameToEdit = null },
                    onSave = { newTitle, newTargets, newCoverPath ->
                        scope.launch {
                            viewModel.editGameOverride(gameItem, newTargets, newCoverPath, newTitle)
                            gameToEdit = null
                        }
                    },
                )
            }

            // ── Save Setup Dialog (Pre-Launch or Post-Game) ──────────
            uiState.saveSetupRequest?.let { request ->
                EditGameDialog(
                    game = request.item,
                    notice = request.notice,
                    cloudRestoreFileNames = request.cloudFileNames,
                    onDismiss = { viewModel.completeSaveSetup(emptyList(), request = request) },
                    onSave = { newTitle, newTargets, newCoverPath ->
                        viewModel.completeSaveSetup(newTargets, newTitle, newCoverPath, request)
                    },
                )
            }

            // ── Update Dialog ──────────────────────────────────────
            if (uiState.showUpdateDialog) {
                UpdateDialog(
                    state = uiState.updateState,
                    onDismiss = { viewModel.dismissUpdateDialog() },
                    onStartDownload = { info ->
                        scope.launch { viewModel.downloadAndApplyUpdate(info) }
                    },
                    onRestart = { file ->
                        viewModel.restartApp(file)
                    },
                )
            }

            // ── Drive Setup Dialog ──────────────────────────────────
            if (showDriveSetupDialog) {
                DriveSetupDialog(
                    initialConfig = uiState.config?.googleDrive,
                    configFile = viewModel.configFile,
                    onDismiss = { showDriveSetupDialog = false },
                    onSaveAndConnect = { clientId, clientSecret ->
                        scope.launch {
                            val result =
                                viewModel.setupGoogleDrive(
                                    clientId,
                                    clientSecret,
                                    connectNow = true,
                                )
                            steamMessage = result
                        }
                    },
                    onUnlink = {
                        scope.launch {
                            val result = viewModel.unlinkGoogleDrive()
                            steamMessage = result
                        }
                    },
                )
            }

            // ── Steam Registration Snackbar ─────────────────────────
            steamMessage?.let { message ->
                val snackbarHostState = remember { SnackbarHostState() }
                LaunchedEffect(message) {
                    snackbarHostState.showSnackbar(
                        message = message,
                        duration = SnackbarDuration.Long,
                    )
                    steamMessage = null
                }
                Box(
                    modifier = Modifier.fillMaxSize(),
                    contentAlignment = Alignment.BottomCenter,
                ) {
                    SnackbarHost(hostState = snackbarHostState)
                }
            }
        }
    }
}

private fun loadAppIcon(): Painter? {
    return try {
        val stream =
            Thread.currentThread().contextClassLoader.getResourceAsStream("icon.png")
                ?: object {}.javaClass.getResourceAsStream("/icon.png")
        val bytes = stream?.use { it.readAllBytes() } ?: return null
        val imageBitmap = Image.makeFromEncoded(bytes).toComposeImageBitmap()
        BitmapPainter(imageBitmap)
    } catch (_: Throwable) {
        null
    }
}
