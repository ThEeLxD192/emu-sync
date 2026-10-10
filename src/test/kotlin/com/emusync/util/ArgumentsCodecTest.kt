package com.emusync.util

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class ArgumentsCodecTest {
    @Test
    fun `format and parse preserve arguments containing spaces`() {
        val arguments = listOf("value with spaces", "--flag", "", "quoted \"value\"", "C:\\folder name\\")

        assertEquals(arguments, ArgumentsCodec.parse(ArgumentsCodec.format(arguments)))
    }

    @Test
    fun `parse supports quoted arguments and whitespace separators`() {
        assertEquals(
            listOf("--profile", "My Profile", "--fullscreen"),
            ArgumentsCodec.parse("  --profile   \"My Profile\"  --fullscreen  ")
        )
    }

    @Test
    fun `parse keeps Windows path separators inside quotes`() {
        assertEquals(listOf("C:\\Games\\My Game"), ArgumentsCodec.parse("\"C:\\Games\\My Game\""))
    }

    @Test
    fun `parse rejects an unmatched quote`() {
        assertNull(ArgumentsCodec.parse("--profile \"unfinished value"))
    }
}
