package com.emusync.util

/** Converts the editable arguments field to and from an argument list without losing spaces. */
object ArgumentsCodec {
    fun format(arguments: List<String>): String = arguments.joinToString(" ") { argument ->
        if (argument.isEmpty() || argument.any { it.isWhitespace() || it == '"' || it == '\\' }) {
            "\"${argument.replace("\\", "\\\\").replace("\"", "\\\"")}\""
        } else {
            argument
        }
    }

    /** Returns null for an unmatched quote or trailing escape inside a quoted argument. */
    fun parse(text: String): List<String>? {
        val result = mutableListOf<String>()
        val current = StringBuilder()
        var inQuotes = false
        var escaped = false
        var tokenStarted = false

        for (char in text) {
            when {
                escaped -> {
                    if (char != '\\' && char != '"') current.append('\\')
                    current.append(char)
                    escaped = false
                }
                inQuotes && char == '\\' -> escaped = true
                char == '"' -> {
                    inQuotes = !inQuotes
                    tokenStarted = true
                }
                char.isWhitespace() && !inQuotes -> {
                    if (tokenStarted) {
                        result += current.toString()
                        current.clear()
                        tokenStarted = false
                    }
                }
                else -> {
                    current.append(char)
                    tokenStarted = true
                }
            }
        }

        if (inQuotes || escaped) return null
        if (tokenStarted) result += current.toString()
        return result
    }
}
