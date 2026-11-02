import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.painter.BitmapPainter
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.graphics.toComposeImageBitmap
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Window
import androidx.compose.ui.window.WindowPosition
import androidx.compose.ui.window.application
import androidx.compose.ui.window.rememberWindowState
import com.emusync.AppInfo
import com.emusync.composition.DesktopApplication
import com.emusync.ui.EmuSyncApp
import javax.swing.UIManager
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
            val app = remember { DesktopApplication() }
            val viewModel = app.viewModel
            DisposableEffect(app) { onDispose { app.close() } }

            // Load config on first composition
            LaunchedEffect(Unit) {
                viewModel.loadConfig()
            }

            EmuSyncApp(
                viewModel,
                AppInfo.VERSION,
                app.configPath,
                app.formActions,
                app::openConfigDirectory,
                onExit = ::exitApplication,
            )
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
