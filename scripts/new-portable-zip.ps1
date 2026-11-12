<#
.SYNOPSIS
  Zips a folder with standard forward-slash entry names, keeping the folder itself as the root.

.DESCRIPTION
  Compress-Archive and ZipFile.CreateFromDirectory in Windows PowerShell 5.1 store backslash
  paths, which other extractors read wrongly. Each entry is added here with an explicit
  forward-slash name, which behaves the same on every PowerShell and .NET.
#>
param(
    [Parameter(Mandatory)] [string] $Source,
    [Parameter(Mandatory)] [string] $Destination
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

$source = (Resolve-Path $Source).Path.TrimEnd('\')
$root = Split-Path -Leaf $source
$archive = [System.IO.Compression.ZipFile]::Open($Destination, [System.IO.Compression.ZipArchiveMode]::Create)
try {
    # -Name yields each path relative to $source. Cutting a prefix off FullName instead breaks
    # when the folder is given by its 8.3 short name (RUNNER~1) and the children report the
    # long one, because the two prefixes differ in length.
    Get-ChildItem -Path $source -Recurse -File -Name | ForEach-Object {
        [void][System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
            $archive, (Join-Path $source $_), ($root + '/' + $_.Replace('\', '/')),
            [System.IO.Compression.CompressionLevel]::Optimal)
    }
}
finally {
    $archive.Dispose()
}
