package com.emusync.domain.sync

/** The result of comparing a local save file with its cloud counterpart. */
enum class SyncDecision {
    /** Local file is newer → should upload to Drive. */
    UPLOAD_LOCAL,

    /** Cloud file is newer → should download from Drive. */
    DOWNLOAD_CLOUD,

    /** Both have the same modification time → no action needed. */
    IN_SYNC,

    /** Timestamps are too close to call or both modified since last sync → ask the user. */
    CONFLICT,
}
