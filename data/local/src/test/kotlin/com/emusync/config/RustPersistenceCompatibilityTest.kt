package com.emusync.config

import com.emusync.domain.config.OAuthCredentials
import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.sync.CloudSaveState
import com.emusync.local.saves.SyncHistoryStore
import java.io.File
import kotlin.test.assertEquals
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class RustPersistenceCompatibilityTest {
    @Test
    fun `Kotlin reads and writes Rust persistence when exchange directory is supplied`(
        @TempDir temporary: File
    ) {
        val exchange = System.getenv("EMUSYNC_R3_EXCHANGE")?.let(::File) ?: temporary
        val file = File(exchange, "config.json")
        if (!file.exists()) {
            file.writeText(
                """{"googleDrive":{"clientId":"rust","clientSecret":"secret"},"entries":[{"type":"emulator","name":"Rust system","executablePath":"emu","romsDirectory":"roms","extensions":["gb"],"arguments":[]}]}"""
            )
            SyncHistoryStore(file)
                .record("rust", "rust-hash", CloudSaveState("remote", "2026-10-01T12:00:00Z"))
        }
        val manager = ConfigManager(file)
        val config = manager.load(autoCreate = false)
        assertEquals("rust", config.cloudCredentials?.clientId)
        assertEquals(emptyList(), (config.entries.first() as EmulatorSystem).arguments)
        val history = SyncHistoryStore(file)
        assertEquals("rust-hash", history.get("rust")?.localSha256)
        manager.save(
            config.copy(
                cloudCredentials = OAuthCredentials("kotlin", "secret"),
                entries = config.entries + NativePCGame("Kotlin game", "game"),
            )
        )
        history.record("kotlin", "kotlin-hash", CloudSaveState("remote", "2026-10-01T12:00:00Z"))
    }
}
