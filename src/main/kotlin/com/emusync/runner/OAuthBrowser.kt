package com.emusync.runner

import com.emusync.domain.operation.recoverOperationFailure
import java.awt.Desktop
import java.net.URI

internal fun openOAuthBrowser(authUrl: String) {
    val desktopBrowse = {
        if (
            Desktop.isDesktopSupported() && Desktop.getDesktop().isSupported(Desktop.Action.BROWSE)
        ) {
            Desktop.getDesktop().browse(URI(authUrl))
        } else if (System.getProperty("os.name", "").lowercase().contains("windows")) {
            windowsBrowser(authUrl)
        } else {
            Runtime.getRuntime().exec(arrayOf("xdg-open", authUrl))
        }
        Unit
    }
    if (System.getProperty("os.name", "").lowercase().contains("windows")) {
        recoverOperationFailure(
            action = desktopBrowse,
            recover = {
                recoverOperationFailure(
                    action = { windowsBrowser(authUrl) },
                    recover = {
                        ProcessBuilder("cmd", "/c", "start", "\"\"", authUrl.replace("&", "^&"))
                            .start()
                        Unit
                    },
                )
            },
        )
    } else desktopBrowse()
}

private fun windowsBrowser(authUrl: String) {
    ProcessBuilder("rundll32", "url.dll,FileProtocolHandler", authUrl).start()
}
