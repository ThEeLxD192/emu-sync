package com.emusync.drive

/** Byte progress emitted by the transport. */
data class TransferProgress(
    val direction: TransferDirection,
    val fileName: String,
    val transferredBytes: Long,
    val totalBytes: Long?,
)
