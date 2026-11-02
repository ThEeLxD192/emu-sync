package com.emusync.composition

import com.emusync.AppInfo
import com.emusync.domain.update.UpdateFiles
import com.emusync.domain.update.UpdateInstaller
import com.emusync.domain.update.UpdateSource
import com.emusync.update.DesktopUpdateInstaller
import com.emusync.update.GitHubUpdateSource
import com.emusync.update.UpdateStagingFiles
import com.emusync.update.createUpdateClient

internal class DesktopUpdates(
    val source: UpdateSource,
    val files: UpdateFiles,
    val installer: UpdateInstaller,
    private val closeResources: () -> Unit = {},
) : AutoCloseable {
    override fun close() = closeResources()
}

internal fun composeDesktopUpdates(): DesktopUpdates {
    val client = createUpdateClient()
    val installer = DesktopUpdateInstaller()
    return DesktopUpdates(
        GitHubUpdateSource(
            client,
            AppInfo.GITHUB_OWNER,
            AppInfo.GITHUB_REPO,
            AppInfo.VERSION,
            installer.isWindows(),
        ),
        UpdateStagingFiles(installer::isWindows, installer::getCurrentAppImagePath),
        installer,
        client::close,
    )
}
