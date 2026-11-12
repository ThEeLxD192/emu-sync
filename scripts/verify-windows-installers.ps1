<#
.SYNOPSIS
  Installs, upgrades, reinstalls and removes the EmuSync MSI and checks the portable ZIP.

.DESCRIPTION
  Changes the machine: it installs and removes EmuSync system-wide, so run it only on a
  disposable Windows machine or in CI. User data under %APPDATA%\emusync is checked, not
  removed, but the script creates a placeholder there if the folder is absent.

.PARAMETER Msi          The MSI built from this repository.
.PARAMETER Zip          The portable ZIP built from this repository.
.PARAMETER Version      The version the MSI must report, for example 2.0.5.
.PARAMETER PreviousMsi  An older MSI that shares the UpgradeCode, to test upgrading from it.
#>
param(
    [Parameter(Mandatory)] [string] $Msi,
    [Parameter(Mandatory)] [string] $Zip,
    [Parameter(Mandatory)] [string] $Version,
    [string] $PreviousMsi
)
$ErrorActionPreference = 'Stop'

$installDirectory = Join-Path $env:ProgramFiles 'EmuSync'
$executable = Join-Path $installDirectory 'EmuSync.exe'
$shortcut = Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\EmuSync\EmuSync.lnk'
$userData = Join-Path $env:APPDATA 'emusync'
$logs = Join-Path (Get-Location) 'installer-logs'
New-Item -ItemType Directory -Force -Path $logs | Out-Null

function Assert-That([bool] $condition, [string] $message) {
    if (-not $condition) { throw "FAILED: $message" }
    Write-Host "ok: $message"
}

function Get-Installed {
    $keys = 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
            'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
    @(Get-ItemProperty $keys -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -eq 'EmuSync' })
}

function Invoke-Msi([string] $label, [string[]] $arguments) {
    $log = Join-Path $logs "$label.log"
    $process = Start-Process msiexec -ArgumentList ($arguments + @('/qn', '/norestart', '/l*v', "`"$log`"")) `
        -Wait -PassThru
    # 3010 means the install succeeded and asks for a restart.
    Assert-That ($process.ExitCode -in 0, 3010) "$label finished with exit code $($process.ExitCode)"
}

function Show-InstallDirectory {
    if (Test-Path $installDirectory) {
        Get-ChildItem $installDirectory -Recurse -Force | ForEach-Object { Write-Host "  $($_.FullName)" }
    }
}

# User data must survive every step.
New-Item -ItemType Directory -Force -Path $userData | Out-Null
$marker = Join-Path $userData 'installer-test-marker.txt'
Set-Content -Path $marker -Value 'user data'

Assert-That (@(Get-Installed).Count -eq 0) 'EmuSync is not installed before the test'

if ($PreviousMsi) {
    Invoke-Msi 'install-previous' @('/i', "`"$PreviousMsi`"")
    $previous = @(Get-Installed)
    Assert-That ($previous.Count -eq 1) 'the previous release registers exactly one product'
    Assert-That (Test-Path $executable) 'the previous release installs EmuSync.exe where this package does'
    Write-Host "previous version: $($previous[0].DisplayVersion)"
    Assert-That ($previous[0].DisplayVersion -ne $Version) 'the previous release has a different version'
}

Invoke-Msi 'install' @('/i', "`"$Msi`"")
$installed = @(Get-Installed)
Assert-That ($installed.Count -eq 1) 'exactly one EmuSync product is registered (the old one was upgraded away)'
Assert-That ($installed[0].DisplayVersion -eq $Version) "the registered version is $Version"
Assert-That (Test-Path $executable) 'EmuSync.exe is installed'
$info = (Get-Item $executable).VersionInfo
Assert-That ($info.ProductName -eq 'EmuSync') 'the installed executable carries the product name'
Assert-That ($info.FileVersion.StartsWith($Version)) "the installed executable reports version $Version"
Assert-That (Test-Path $shortcut) 'the Start menu shortcut exists'
if ($PreviousMsi) {
    $leftovers = @(Get-ChildItem $installDirectory -Force | Where-Object { $_.Name -ne 'EmuSync.exe' })
    if ($leftovers.Count -ne 0) { Show-InstallDirectory }
    Assert-That ($leftovers.Count -eq 0) 'nothing of the previous release is left in the install folder'
}
Assert-That (Test-Path $marker) 'user data survived the install'

Invoke-Msi 'reinstall' @('/i', "`"$Msi`"")
Assert-That (@(Get-Installed).Count -eq 1) 'reinstalling the same version keeps one product'
Assert-That (Test-Path $executable) 'EmuSync.exe is still installed after reinstalling'

$code = @(Get-Installed)[0].PSChildName
Invoke-Msi 'uninstall' @('/x', $code)
Assert-That (@(Get-Installed).Count -eq 0) 'uninstalling removes the registration'
Assert-That (-not (Test-Path $executable)) 'uninstalling removes EmuSync.exe'
Assert-That (-not (Test-Path $shortcut)) 'uninstalling removes the Start menu shortcut'
Assert-That (Test-Path $marker) 'user data survived the uninstall'

# Portable package: executable, marker and license, nothing machine-wide.
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [System.IO.Compression.ZipFile]::OpenRead($Zip)
$backslashed = @($archive.Entries | Where-Object { $_.FullName.Contains('\') })
$archive.Dispose()
if ($backslashed.Count -ne 0) { $backslashed | ForEach-Object { Write-Host "  backslash in entry: $($_.FullName)" } }
Assert-That ($backslashed.Count -eq 0) 'the ZIP uses forward slashes, so any extractor reads it'

$extracted = Join-Path (Get-Location) 'portable-check'
if (Test-Path $extracted) { Remove-Item -Recurse -Force $extracted }
Expand-Archive -Path $Zip -DestinationPath $extracted
$folder = Join-Path $extracted 'EmuSync'
foreach ($name in 'EmuSync.exe', 'portable.txt', 'LICENSE') {
    Assert-That (Test-Path (Join-Path $folder $name)) "the portable ZIP contains $name"
}
Assert-That (((Get-ChildItem $folder).Count) -eq 3) 'the portable ZIP contains nothing else'
Assert-That ((Get-Item (Join-Path $folder 'EmuSync.exe')).VersionInfo.FileVersion.StartsWith($Version)) `
    "the portable executable reports version $Version"

# The executable itself: no console window and no dependency on the Visual C++ runtime.
$bytes = [System.IO.File]::ReadAllBytes((Join-Path $folder 'EmuSync.exe'))
$pe = [BitConverter]::ToInt32($bytes, 0x3C)
# Subsystem sits 68 bytes into the optional header, after the 4-byte signature and the 20-byte
# COFF header: 2 is a GUI application, 3 would open a console window.
$subsystem = [BitConverter]::ToUInt16($bytes, $pe + 24 + 68)
Assert-That ($subsystem -eq 2) "the executable is a GUI application (subsystem $subsystem)"
Assert-That (-not ([System.Text.Encoding]::ASCII.GetString($bytes) -match 'VCRUNTIME140')) `
    'the executable links the C runtime statically'

Remove-Item -Force $marker
Write-Host 'All installer checks passed.'
