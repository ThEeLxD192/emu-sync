package com.emusync.steam

import com.emusync.model.EmulatorSystem
import com.emusync.util.ArgumentsCodec
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.io.File
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.nio.file.AtomicMoveNotSupportedException
import java.util.Base64

@Serializable
internal data class WindowsLaunchConfiguration(val executable: String, val signalFile: String, val arguments: List<String>)

/** Arguments live in JSON, never in cmd.exe source or an evaluated PowerShell expression. */
internal class WindowsSteamLauncher(private val directory: File) {
    fun create(entry: EmulatorSystem, key: String, signal: File): File {
        val fullscreen = requireNotNull(ArgumentsCodec.parse(SteamShortcutManager.resolveFullscreenArgs(entry))) {
            "Fullscreen arguments contain unmatched quotes"
        }
        require((listOf(entry.executablePath, signal.path) + fullscreen + entry.arguments).none { '\u0000' in it }) {
            "Launcher paths and arguments cannot contain NUL"
        }
        writeLauncherFile(File(directory, "windows-launcher-v1.ps1"), SCRIPT)
        writeLauncherFile(File(directory, "launch_$key.json"), Json.encodeToString(WindowsLaunchConfiguration.serializer(),
            WindowsLaunchConfiguration(entry.executablePath, signal.absolutePath, fullscreen + entry.arguments)))
        return File(directory, "launch_$key.bat").also { writeLauncherFile(it, batch(encodedCommand(key))) }
    }

    fun encodedCommand(key: String): String {
        fun path(file: File) = Base64.getEncoder().encodeToString(file.absolutePath.toByteArray(Charsets.UTF_8))
        val script = "& ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('${path(File(directory, "windows-launcher-v1.ps1"))}'))) " +
            "-ConfigPath ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('${path(File(directory, "launch_$key.json"))}')))\nexit \$LASTEXITCODE"
        return Base64.getEncoder().encodeToString(script.toByteArray(Charsets.UTF_16LE))
    }

    fun batch(encoded: String): String = "@echo off\r\nsetlocal DisableDelayedExpansion\r\npowershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand $encoded\r\nexit /b %ERRORLEVEL%\r\n"

    companion object {
        internal val SCRIPT = """
        param([Parameter(Mandatory=${'$'}true)][string]${'$'}ConfigPath)
        ${'$'}ErrorActionPreference = 'Stop'
        Set-StrictMode -Version 2.0

        # Serialize a native Windows argument according to the Microsoft CRT rules.
        function ConvertTo-NativeArgument([AllowEmptyString()][string]${'$'}Value) {
            ${'$'}builder = New-Object System.Text.StringBuilder
            [void]${'$'}builder.Append('"')
            ${'$'}slashes = 0
            foreach (${'$'}character in ${'$'}Value.ToCharArray()) {
                if (${'$'}character -eq '\') { ${'$'}slashes++; continue }
                if (${'$'}character -eq '"') {
                    [void]${'$'}builder.Append(('\' * (2 * ${'$'}slashes + 1)))
                } else {
                    [void]${'$'}builder.Append(('\' * ${'$'}slashes))
                }
                [void]${'$'}builder.Append(${'$'}character)
                ${'$'}slashes = 0
            }
            [void]${'$'}builder.Append(('\' * (2 * ${'$'}slashes)))
            [void]${'$'}builder.Append('"')
            return ${'$'}builder.ToString()
        }

        try {
            ${'$'}configuration = [IO.File]::ReadAllText(${'$'}ConfigPath, [Text.Encoding]::UTF8) | ConvertFrom-Json
            if (${'$'}configuration.executable -match '(?i)\.(bat|cmd)${'$'}') {
                throw 'Choose an emulator executable instead of a batch script.'
            }
            if (!(Test-Path -LiteralPath ${'$'}configuration.signalFile -PathType Leaf)) {
                throw 'No ROM path found. Launch a game from EmuSync first.'
            }
            ${'$'}romPath = [IO.File]::ReadAllText(${'$'}configuration.signalFile, [Text.Encoding]::UTF8).TrimEnd([char[]]"`r`n")
            if (!(Test-Path -LiteralPath ${'$'}romPath)) { throw 'The selected ROM no longer exists.' }
            ${'$'}nativeArguments = @(${'$'}configuration.arguments | ForEach-Object {
                ConvertTo-NativeArgument (([string]${'$'}_).Replace('{ROM}', ${'$'}romPath))
            })
            ${'$'}start = New-Object Diagnostics.ProcessStartInfo
            ${'$'}start.FileName = ${'$'}configuration.executable
            ${'$'}start.UseShellExecute = ${'$'}false
            ${'$'}start.Arguments = ${'$'}nativeArguments -join ' '
            ${'$'}workingDirectory = [IO.Path]::GetDirectoryName(${'$'}configuration.executable)
            if (${'$'}workingDirectory -and (Test-Path -LiteralPath ${'$'}workingDirectory -PathType Container)) {
                ${'$'}start.WorkingDirectory = ${'$'}workingDirectory
            }
            ${'$'}process = [Diagnostics.Process]::Start(${'$'}start)
            ${'$'}process.WaitForExit()
            ${'$'}exitCode = ${'$'}process.ExitCode
            ${'$'}process.Dispose()
            exit ${'$'}exitCode
        } catch {
            [Console]::Error.WriteLine(${'$'}_.Exception.Message)
            exit 1
        }
        """.trimIndent() + "\n"
    }
}

internal fun writeLauncherFile(file: File, contents: String) {
    Files.createDirectories(file.absoluteFile.parentFile.toPath())
    val staged = Files.createTempFile(file.absoluteFile.parentFile.toPath(), ".${file.name}.", ".tmp")
    try {
        Files.writeString(staged, contents)
        try {
            Files.move(staged, file.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
        } catch (_: AtomicMoveNotSupportedException) {
            Files.move(staged, file.toPath(), StandardCopyOption.REPLACE_EXISTING)
        }
    } finally { Files.deleteIfExists(staged) }
}
