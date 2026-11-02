package com.emusync.domain.sync

/** Determines how a newer local copy is handled by [planSyncDecision]. */
enum class LocalNewerAction {
    /** Ask for conflict resolution when the local copy is clearly newer. */
    RESOLVE_CONFLICT,

    /** Prefer local progress, as in the post-play upload step. */
    UPLOAD,
}
