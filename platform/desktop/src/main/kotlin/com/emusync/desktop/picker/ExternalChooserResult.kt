package com.emusync.desktop.picker

internal sealed interface ExternalChooserResult {
    data class Selected(val path: String) : ExternalChooserResult

    object Cancelled : ExternalChooserResult

    object NotAvailable : ExternalChooserResult
}
