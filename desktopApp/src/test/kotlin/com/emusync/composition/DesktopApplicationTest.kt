package com.emusync.composition

import com.emusync.config.ConfigManager
import com.emusync.update.DesktopUpdateInstaller
import com.emusync.update.GitHubUpdateSource
import com.emusync.update.UpdateStagingFiles
import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respondOk
import io.ktor.client.request.get
import java.io.File
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlinx.coroutines.job
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class DesktopApplicationTest {
    @Test
    fun `desktop owner closes both HTTP clients after presentation has borrowed them`(
        @TempDir directory: File
    ) = runTest {
        val drive = HttpClient(MockEngine { respondOk() })
        val update = HttpClient(MockEngine { respondOk() })
        val updates =
            DesktopUpdates(
                GitHubUpdateSource(update, "owner", "repo", "1.0.0", false),
                UpdateStagingFiles({ false }, { null }),
                DesktopUpdateInstaller(),
                update::close,
            )
        val app = DesktopApplication(ConfigManager(File(directory, "config.json")), drive, updates)
        try {
            app.viewModel.loadConfig()
            drive.get("https://drive.example/health")
            update.get("https://updates.example/health")
            assertFalse(drive.coroutineContext.job.isCompleted)
            assertFalse(update.coroutineContext.job.isCompleted)
            app.close()
            drive.coroutineContext.job.join()
            update.coroutineContext.job.join()
            assertTrue(drive.coroutineContext.job.isCompleted)
            assertTrue(update.coroutineContext.job.isCompleted)
        } finally {
            app.close()
        }
    }
}
