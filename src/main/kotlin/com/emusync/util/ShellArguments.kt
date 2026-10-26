package com.emusync.util

/** Double-quoted shell literal; only an explicitly requested ROM placeholder is expanded. */
internal fun shellArgument(value: String, expandRom: Boolean = false): String {
    require('\u0000' !in value) { "Shell arguments cannot contain NUL" }
    val escaped = value.replace("\\", "\\\\").replace("\"", "\\\"").replace("$", "\\$").replace("`", "\\`")
    return "\"" + (if (expandRom) escaped.replace("{ROM}", "\${ROM_PATH}") else escaped) + "\""
}
