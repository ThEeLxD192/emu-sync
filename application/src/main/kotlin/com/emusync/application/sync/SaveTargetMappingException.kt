package com.emusync.application.sync

/** Configuration problems must not be treated as an offline pre-sync. */
class SaveTargetMappingException(message: String) : IllegalArgumentException(message)
