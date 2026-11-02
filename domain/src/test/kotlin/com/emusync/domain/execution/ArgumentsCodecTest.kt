package com.emusync.domain.execution

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class ArgumentsCodecTest {
    @Test
    fun `format and parse preserve arguments containing spaces`() {
        val arguments =
            listOf("value with spaces", "--flag", "", "quoted \"value\"", "C:\\folder name\\")

        assertEquals(arguments, ArgumentsCodec.parse(ArgumentsCodec.format(arguments)))
    }

    @Test
    fun `parse supports quoted arguments and whitespace separators`() {
        assertEquals(
            listOf("--profile", "My Profile", "--fullscreen"),
            ArgumentsCodec.parse("  --profile   \"My Profile\"  --fullscreen  "),
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

    @Test
    fun `parse decodes escaped quotes and backslashes but preserves other escapes`() {
        assertEquals(
            listOf("a\"b", "a\\b", "a\\qb"),
            ArgumentsCodec.parse("\"a\\\"b\" \"a\\\\b\" \"a\\qb\""),
        )
    }

    @Test
    fun `parse rejects a trailing escape inside a quoted argument`() {
        assertNull(ArgumentsCodec.parse("\"unfinished\\"))
    }

    @Test
    fun `parse preserves empty arguments and joins adjacent quoted and unquoted fragments`() {
        assertEquals(
            listOf("", "prefix with spaces suffix", ""),
            ArgumentsCodec.parse("\"\" prefix\" with spaces \"suffix \"\""),
        )
    }

    @Test
    fun `format quotes only arguments that require it`() {
        assertEquals("plain --flag", ArgumentsCodec.format(listOf("plain", "--flag")))
        assertEquals("\"\"", ArgumentsCodec.format(listOf("")))
        assertEquals("\"a b\"", ArgumentsCodec.format(listOf("a b")))
        assertEquals("\"a\tb\"", ArgumentsCodec.format(listOf("a\tb")))
        assertEquals("\"a\\\"b\"", ArgumentsCodec.format(listOf("a\"b")))
        assertEquals("\"a\\\\b\"", ArgumentsCodec.format(listOf("a\\b")))
    }
}
