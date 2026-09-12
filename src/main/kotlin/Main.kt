import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Cloud
import androidx.compose.material.icons.filled.CloudDone
import androidx.compose.material.icons.filled.CloudOff
import androidx.compose.material.icons.filled.CloudSync
import androidx.compose.material.icons.filled.RestartAlt
import androidx.compose.material.icons.filled.SystemUpdate
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.painter.BitmapPainter
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.graphics.toComposeImageBitmap
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Window
import androidx.compose.ui.window.WindowPosition
import androidx.compose.ui.window.application
import androidx.compose.ui.window.rememberWindowState
import com.emusync.AppInfo
import com.emusync.config.ConfigManager
import com.emusync.model.GameEntry
import com.emusync.ui.AppViewModel
import com.emusync.ui.CloudSyncStatus
import com.emusync.ui.GameItem
import com.emusync.ui.UpdateUiState
import com.emusync.ui.components.GameGrid
import com.emusync.ui.components.Sidebar
import com.emusync.ui.dialogs.AddEntryDialog
import com.emusync.ui.dialogs.EditGameDialog
import com.emusync.ui.dialogs.StatusOverlay
import com.emusync.ui.dialogs.UpdateDialog
import com.emusync.ui.theme.EmuSyncColors
import com.emusync.ui.theme.EmuSyncDarkScheme
import kotlinx.coroutines.launch
import org.jetbrains.skia.Image
import javax.swing.UIManager

fun main() {
    try {
        UIManager.setLookAndFeel(UIManager.getSystemLookAndFeelClassName())
    } catch (_: Exception) {
        // Fallback to default if system L&F fails
    }
    
    application {
        val windowState = rememberWindowState(
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
            // Force 1:1 dp-to-pixel mapping so gamescope's inflated DPI
            // doesn't cause the UI to render at 2× on the Steam Deck.
            CompositionLocalProvider(LocalDensity provides Density(1f, 1f)) {
                val viewModel = remember { AppViewModel(ConfigManager("config.json")) }

                // Load config on first composition
                LaunchedEffect(Unit) {
                    viewModel.loadConfig()
                }

                EmuSyncApp(viewModel, onExit = ::exitApplication)
            }
        }
    }
}

@Composable
fun EmuSyncApp(viewModel: AppViewModel, onExit: () -> Unit) {
    val uiState by viewModel.uiState.collectAsState()
    val scope = rememberCoroutineScope()
    var showAddDialog by remember { mutableStateOf(false) }
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
                // ── Header ──────────────────────────────────────────
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 20.dp, vertical = 14.dp),
                ) {
                    Text(
                        text = "EmuSync",
                        style = MaterialTheme.typography.titleLarge,
                        fontWeight = FontWeight.Bold,
                        color = EmuSyncColors.Primary,
                        letterSpacing = 0.5.sp,
                        modifier = Modifier.align(Alignment.CenterStart),
                    )
                    Row(
                        modifier = Modifier.align(Alignment.CenterEnd),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        // Google Drive status indicator / Connect action
                        val googleDrive = uiState.config?.googleDrive
                        if (googleDrive != null) {
                            val isLinked = !googleDrive.refreshToken.isNullOrBlank()
                            if (isLinked) {
                                Surface(
                                    shape = RoundedCornerShape(8.dp),
                                    color = EmuSyncColors.SurfaceVariant,
                                    modifier = Modifier.padding(end = 12.dp)
                                ) {
                                    Row(
                                        modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                                        verticalAlignment = Alignment.CenterVertically
                                    ) {
                                        Icon(
                                            imageVector = Icons.Default.Cloud,
                                            contentDescription = "Drive Connected",
                                            tint = EmuSyncColors.Success,
                                            modifier = Modifier.size(16.dp)
                                        )
                                        Spacer(Modifier.width(6.dp))
                                        Text(
                                            text = "Drive Linked",
                                            style = MaterialTheme.typography.labelSmall,
                                            color = EmuSyncColors.OnSurface
                                        )
                                    }
                                }
                            } else {
                                OutlinedButton(
                                    onClick = {
                                        scope.launch {
                                            val result = viewModel.loginGoogleDrive()
                                            steamMessage = result
                                        }
                                    },
                                    contentPadding = PaddingValues(horizontal = 12.dp, vertical = 6.dp),
                                    modifier = Modifier.padding(end = 12.dp)
                                ) {
                                    Icon(
                                        imageVector = Icons.Default.CloudSync,
                                        contentDescription = "Connect Google Drive",
                                        tint = EmuSyncColors.Primary,
                                        modifier = Modifier.size(16.dp)
                                    )
                                    Spacer(Modifier.width(6.dp))
                                    Text("Connect Drive", style = MaterialTheme.typography.labelSmall)
                                }
                            }
                        }

                        // Update notification chip
                        val updateState = uiState.updateState
                        if (updateState is UpdateUiState.Available) {
                            Surface(
                                onClick = { viewModel.showUpdateDialog() },
                                shape = RoundedCornerShape(8.dp),
                                color = EmuSyncColors.Primary,
                                modifier = Modifier.padding(end = 12.dp)
                            ) {
                                Row(
                                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                                    verticalAlignment = Alignment.CenterVertically
                                ) {
                                    Icon(
                                        imageVector = Icons.Default.SystemUpdate,
                                        contentDescription = null,
                                        tint = Color.White,
                                        modifier = Modifier.size(16.dp)
                                    )
                                    Spacer(Modifier.width(6.dp))
                                    Text(
                                        text = "Update v${updateState.info.version}",
                                        style = MaterialTheme.typography.labelSmall,
                                        color = Color.White,
                                        fontWeight = FontWeight.Bold
                                    )
                                }
                            }
                        } else if (updateState is UpdateUiState.ReadyToRestart) {
                            Surface(
                                onClick = { viewModel.showUpdateDialog() },
                                shape = RoundedCornerShape(8.dp),
                                color = EmuSyncColors.Success,
                                modifier = Modifier.padding(end = 12.dp)
                            ) {
                                Row(
                                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                                    verticalAlignment = Alignment.CenterVertically
                                ) {
                                    Icon(
                                        imageVector = Icons.Default.RestartAlt,
                                        contentDescription = null,
                                        tint = Color.White,
                                        modifier = Modifier.size(16.dp)
                                    )
                                    Spacer(Modifier.width(6.dp))
                                    Text(
                                        text = "Restart EmuSync",
                                        style = MaterialTheme.typography.labelSmall,
                                        color = Color.White,
                                        fontWeight = FontWeight.Bold
                                    )
                                }
                            }
                        }

                        Text(
                            text = "v${AppInfo.VERSION}",
                            style = MaterialTheme.typography.labelSmall,
                            color = EmuSyncColors.OnSurfaceDim,
                            modifier = Modifier.padding(end = 16.dp),
                        )
                        Button(
                            onClick = onExit,
                            colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error),
                            contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp)
                        ) {
                            Text("Safe Close")
                        }
                    }
                }

                HorizontalDivider(
                    color = EmuSyncColors.Divider,
                    thickness = 1.dp,
                )

                // ── Main Content: Sidebar + Game Grid ───────────────
                Row(modifier = Modifier.fillMaxSize()) {
                    // Left sidebar (categories)
                    Sidebar(
                        entries = uiState.config?.entries ?: emptyList(),
                        selectedEntry = uiState.selectedEntry,
                        onEntrySelected = { entry ->
                            scope.launch { viewModel.selectEntry(entry) }
                        },
                        onAddClicked = { showAddDialog = true },
                        onReorder = { fromIndex, toIndex ->
                            scope.launch { viewModel.reorderEntries(fromIndex, toIndex) }
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
                        modifier = Modifier
                            .width(240.dp)
                            .fillMaxHeight(),
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
                        syncStatus = uiState.entrySyncStatus[uiState.selectedEntry?.name] ?: CloudSyncStatus.IDLE,
                        onGameClicked = { gameItem ->
                            scope.launch { viewModel.launchGame(gameItem) }
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
                        modifier = Modifier
                            .weight(1f)
                            .fillMaxHeight()
                            .padding(20.dp),
                    )
                }
            }

            // ── Status Overlays (on top of everything) ──────────────
            StatusOverlay(
                status = uiState.status,
                onDismissError = { viewModel.clearStatus() }
            )

            // ── Add Entry Dialog ────────────────────────────────────
            if (showAddDialog) {
                AddEntryDialog(
                    initialEntry = entryToEdit,
                    onDismiss = { 
                        showAddDialog = false
                        entryToEdit = null
                    },
                    onSave = { newEntry ->
                        showAddDialog = false
                        scope.launch {
                            if (entryToEdit == null) {
                                viewModel.addEntry(newEntry)
                            } else {
                                viewModel.editEntry(entryToEdit!!, newEntry)
                            }
                            entryToEdit = null
                        }
                    }
                )
            }

            // ── Edit Game Save Dialog (Manual Edit) ─────────────────
            gameToEdit?.let { gameItem ->
                EditGameDialog(
                    game = gameItem,
                    onDismiss = { gameToEdit = null },
                    onSave = { newPaths ->
                        scope.launch {
                            viewModel.editGameOverride(gameItem, newPaths)
                            gameToEdit = null
                        }
                    }
                )
            }

            // ── Post-Game Save Setup Dialog ─────────────────────────
            uiState.saveSetupRequest?.let { gameItem ->
                EditGameDialog(
                    game = gameItem,
                    onDismiss = { viewModel.completeSaveSetup(emptyList()) },
                    onSave = { newPaths -> viewModel.completeSaveSetup(newPaths) }
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
                    }
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
        val stream = Thread.currentThread().contextClassLoader.getResourceAsStream("icon.png")
            ?: object {}.javaClass.getResourceAsStream("/icon.png")
        val bytes = stream?.readAllBytes() ?: return null
        val imageBitmap = Image.makeFromEncoded(bytes).toComposeImageBitmap()
        BitmapPainter(imageBitmap)
    } catch (_: Throwable) {
        null
    }
}
