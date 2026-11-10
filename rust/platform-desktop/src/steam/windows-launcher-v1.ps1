param([Parameter(Mandatory=$true)][string]$ConfigPath)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

# Serialize a native Windows argument according to the Microsoft CRT rules.
function ConvertTo-NativeArgument([AllowEmptyString()][string]$Value) {
    $builder = New-Object System.Text.StringBuilder
    [void]$builder.Append('"')
    $slashes = 0
    foreach ($character in $Value.ToCharArray()) {
        if ($character -eq '\') { $slashes++; continue }
        if ($character -eq '"') {
            [void]$builder.Append(('\' * (2 * $slashes + 1)))
        } else {
            [void]$builder.Append(('\' * $slashes))
        }
        [void]$builder.Append($character)
        $slashes = 0
    }
    [void]$builder.Append(('\' * (2 * $slashes)))
    [void]$builder.Append('"')
    return $builder.ToString()
}

try {
    $configuration = [IO.File]::ReadAllText($ConfigPath, [Text.Encoding]::UTF8) | ConvertFrom-Json
    if ($configuration.executable -match '(?i)\.(bat|cmd)$') {
        throw 'Choose an emulator executable instead of a batch script.'
    }
    if (!(Test-Path -LiteralPath $configuration.signalFile -PathType Leaf)) {
        throw 'No ROM path found. Launch a game from EmuSync first.'
    }
    $romPath = [IO.File]::ReadAllText($configuration.signalFile, [Text.Encoding]::UTF8).TrimEnd([char[]]"`r`n")
    if (!(Test-Path -LiteralPath $romPath)) { throw 'The selected ROM no longer exists.' }
    $nativeArguments = @($configuration.arguments | ForEach-Object {
        ConvertTo-NativeArgument (([string]$_).Replace('{ROM}', $romPath))
    })
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = $configuration.executable
    $start.UseShellExecute = $false
    $start.Arguments = $nativeArguments -join ' '
    $workingDirectory = [IO.Path]::GetDirectoryName($configuration.executable)
    if ($workingDirectory -and (Test-Path -LiteralPath $workingDirectory -PathType Container)) {
        $start.WorkingDirectory = $workingDirectory
    }
    $process = [Diagnostics.Process]::Start($start)
    $process.WaitForExit()
    $exitCode = $process.ExitCode
    $process.Dispose()
    exit $exitCode
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
