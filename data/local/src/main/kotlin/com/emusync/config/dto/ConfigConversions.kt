package com.emusync.config.dto

import com.emusync.domain.config.AppConfig
import com.emusync.domain.config.OAuthCredentials
import com.emusync.domain.library.EmulatorSystem
import com.emusync.domain.library.GameEntry
import com.emusync.domain.library.NativePCGame
import com.emusync.domain.library.SaveTarget
import com.emusync.domain.library.SaveTargetType

internal fun ConfigDocument.toConfig() =
    AppConfig(googleDrive?.toDomain(), entries.map { it.toDomain() })

internal fun AppConfig.toDocument() =
    ConfigDocument(cloudCredentials?.toDto(), entries.map { it.toDto() })

private fun SaveTargetTypeDto.toDomain(): SaveTargetType =
    when (this) {
        SaveTargetTypeDto.FILE -> SaveTargetType.FILE
        SaveTargetTypeDto.DIRECTORY -> SaveTargetType.DIRECTORY
        SaveTargetTypeDto.UNKNOWN -> SaveTargetType.UNKNOWN
    }

private fun SaveTargetType.toDto(): SaveTargetTypeDto =
    when (this) {
        SaveTargetType.FILE -> SaveTargetTypeDto.FILE
        SaveTargetType.DIRECTORY -> SaveTargetTypeDto.DIRECTORY
        SaveTargetType.UNKNOWN -> SaveTargetTypeDto.UNKNOWN
    }

private fun SaveTargetDto.toDomain() = SaveTarget(path, type.toDomain(), cloudPath)

private fun SaveTarget.toDto() = SaveTargetDto(path, type.toDto(), cloudPath)

internal fun GameEntryDto.toDomain(): GameEntry =
    when (this) {
        is EmulatorSystemDto ->
            EmulatorSystem(
                name = name,
                executablePath = executablePath,
                arguments = arguments,
                romsDirectory = romsDirectory,
                extensions = extensions,
                savePaths = savePaths,
                saveTargets = saveTargets.map { it.toDomain() },
                savePathsByRom = savePathsByRom,
                saveTargetsByRom =
                    saveTargetsByRom.mapValues { (_, targets) -> targets.map { it.toDomain() } },
                coverPathByRom = coverPathByRom,
                titleByRom = titleByRom,
                group = group,
                cloudFolder = cloudFolder,
                driveFileId = driveFileId,
                steamAppId = steamAppId,
                steamProcessName = steamProcessName,
                fullscreenArgs = fullscreenArgs,
                coversDirectory = coversDirectory,
            )
        is NativePCGameDto ->
            NativePCGame(
                name = name,
                executablePath = executablePath,
                arguments = arguments,
                savePaths = savePaths,
                saveTargets = saveTargets.map { it.toDomain() },
                waitForProcess = waitForProcess,
                group = group,
                cloudFolder = cloudFolder,
                driveFileId = driveFileId,
                steamAppId = steamAppId,
                coverPath = coverPath,
            )
    }

internal fun GameEntry.toDto(): GameEntryDto =
    when (this) {
        is EmulatorSystem ->
            EmulatorSystemDto(
                name = name,
                executablePath = executablePath,
                arguments = arguments,
                romsDirectory = romsDirectory,
                extensions = extensions,
                savePaths = savePaths,
                saveTargets = saveTargets.map { it.toDto() },
                savePathsByRom = savePathsByRom,
                saveTargetsByRom =
                    saveTargetsByRom.mapValues { (_, targets) -> targets.map { it.toDto() } },
                coverPathByRom = coverPathByRom,
                titleByRom = titleByRom,
                group = group,
                cloudFolder = cloudFolder,
                driveFileId = driveFileId,
                steamAppId = steamAppId,
                steamProcessName = steamProcessName,
                fullscreenArgs = fullscreenArgs,
                coversDirectory = coversDirectory,
            )
        is NativePCGame ->
            NativePCGameDto(
                name = name,
                executablePath = executablePath,
                arguments = arguments,
                savePaths = savePaths,
                saveTargets = saveTargets.map { it.toDto() },
                waitForProcess = waitForProcess,
                group = group,
                cloudFolder = cloudFolder,
                driveFileId = driveFileId,
                steamAppId = steamAppId,
                coverPath = coverPath,
            )
    }

private fun GoogleDriveConfigDto.toDomain() = OAuthCredentials(clientId, clientSecret, refreshToken)

private fun OAuthCredentials.toDto() = GoogleDriveConfigDto(clientId, clientSecret, refreshToken)
