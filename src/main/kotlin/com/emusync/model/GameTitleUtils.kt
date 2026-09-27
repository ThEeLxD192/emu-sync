package com.emusync.model

import java.io.File

/**
 * Cleans a raw ROM filename into a display-friendly game title.
 * - Replaces underscores with spaces.
 * - Strips square bracket tags (e.g. `[!]`, `[b1]`, `[T+Spa]`, `[v1.0]`).
 * - Strips parenthesis tags (e.g. `(USA)`, `(Europe)`, `(En,Fr,De)`, `(Rev 1)`),
 *   while preserving multi-disc indicators (e.g. `(Disc 1)`, `(CD 2)`, `(Part 1)`).
 * - Collapses multiple spaces and trims.
 * - If the cleaned name is blank, falls back to the original raw name.
 */
fun cleanGameTitle(rawName: String): String {
    // Strip trailing file extension if present (e.g. .gba, .sfc, .iso, .7z)
    val base = rawName.replace(Regex("\\.[a-zA-Z0-9]{1,5}$"), "")
    var cleaned = base.replace('_', ' ')
    cleaned = cleaned.replace(Regex("\\[[^\\]]*\\]"), "")
    cleaned = cleaned.replace(Regex("\\((?!(?:Disc|CD|Part)\\s*\\d+)[^)]*\\)", RegexOption.IGNORE_CASE), "")
    cleaned = cleaned.replace(Regex("\\s+"), " ").trim()
    return cleaned.ifBlank { rawName.trim() }
}

/**
 * Normalizes a game filename or title into a canonical matching key.
 * Used to match games across different installations where ROMs might have slightly different names
 * (e.g. "Super_Mario_World_[U].sfc" vs "Super Mario World (USA).sfc").
 */
fun normalizeGameKey(name: String): String {
    val cleaned = cleanGameTitle(name)
    return cleaned.lowercase()
        .replace(Regex("[^a-z0-9]"), " ")
        .replace(Regex("\\s+"), " ")
        .trim()
}
