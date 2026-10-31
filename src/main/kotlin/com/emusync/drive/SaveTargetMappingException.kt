package com.emusync.drive

/** Configuration problems must not be treated as an offline pre-sync. */
internal class SaveTargetMappingException(message: String) : IllegalArgumentException(message)
