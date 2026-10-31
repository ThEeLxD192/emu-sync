package com.emusync.steam

import java.io.ByteArrayOutputStream
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir

class BinaryVdfFieldTest {
    @Test
    fun `legacy aliases remain fallbacks and empty canonical fields retain precedence`(
        @TempDir directory: File
    ) {
        val alias = mapOf("appname" to "Legacy", "exe" to "legacy.exe", "startdir" to "legacy")
        val legacy = readFields(directory, alias)
        assertEquals("Legacy", legacy.appName)
        assertEquals("legacy.exe", legacy.exe)
        assertEquals("legacy", legacy.startDir)
        assertTrue(legacy.allowDesktopConfig)
        assertTrue(legacy.allowOverlay)

        val canonical =
            readFields(
                directory,
                alias + mapOf("AppName" to "", "Exe" to "new.exe", "StartDir" to "new"),
            )
        assertEquals("", canonical.appName)
        assertEquals("new.exe", canonical.exe)
        assertEquals("new", canonical.startDir)

        assertEquals("Legacy", readFields(directory, alias + ("AppName" to 123)).appName)
    }

    private fun readFields(directory: File, fields: Map<String, Any>): SteamShortcut {
        val output = ByteArrayOutputStream()
        fun text(value: String) {
            output.write(value.toByteArray(Charsets.UTF_8))
            output.write(0)
        }
        output.write(0)
        text("shortcuts")
        output.write(0)
        text("0")
        fields.forEach { (key, value) ->
            output.write(if (value is Int) 2 else 1)
            text(key)
            if (value is Int)
                output.write(
                    ByteBuffer.allocate(4).order(ByteOrder.LITTLE_ENDIAN).putInt(value).array()
                )
            else text(value.toString())
        }
        repeat(3) { output.write(8) }
        val file = File(directory, "shortcuts.vdf").apply { writeBytes(output.toByteArray()) }
        return BinaryVdf.read(file).single()
    }
}
