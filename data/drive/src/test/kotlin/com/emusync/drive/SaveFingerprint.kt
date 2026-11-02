package com.emusync.drive

import java.security.MessageDigest

internal fun sha256(bytes: ByteArray): String =
    MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") {
        "%02x".format(java.util.Locale.ROOT, it)
    }
