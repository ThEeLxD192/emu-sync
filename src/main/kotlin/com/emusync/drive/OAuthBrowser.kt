package com.emusync.drive

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
        recoverDriveFailure(
            action = desktopBrowse,
            recover = {
                recoverDriveFailure(
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
