# backup-archive for Windows - does what archive.sh does (BUG-294).
#
# This file is ASCII on purpose: Windows PowerShell 5.1 reads a script without a
# BOM in the system code page, so non-ASCII text here could break the parse.
#
# stdin: the payload from transform.rhai - the snapshot path as a JSON string.
# The working directory is the plugin data folder, not this folder (BUG-279).
# Settings (ARCHIVE_DIR, ...) arrive as environment variables.
$ErrorActionPreference = 'Stop'

# stdin is UTF-8. PowerShell 5.1 would decode it with the console code page
# and mangle non-ASCII paths, so read the raw stream ourselves.
$reader = New-Object System.IO.StreamReader([Console]::OpenStandardInput(), (New-Object System.Text.UTF8Encoding($false)))
$raw = $reader.ReadToEnd().Trim()
if ($raw -eq '') { exit 0 }

# A JSON string: backslashes arrive escaped. Wrap it in an array so that
# PowerShell 5.1 parses a top-level scalar too.
$src = @(ConvertFrom-Json ('[' + $raw + ']'))[0]
if ([string]::IsNullOrEmpty($src)) { exit 0 }

$dir = $env:ARCHIVE_DIR
if ([string]::IsNullOrEmpty($dir)) {
    [Console]::Error.WriteLine('ARCHIVE_DIR is empty')
    exit 1
}
if (-not [System.IO.File]::Exists($src)) {
    [Console]::Error.WriteLine("backup file not found: $src")
    exit 1
}

[System.IO.Directory]::CreateDirectory($dir) | Out-Null
# Never overwrite a copy that is already there.
$dest = [System.IO.Path]::Combine($dir, [System.IO.Path]::GetFileName($src))
if ([System.IO.File]::Exists($dest)) { exit 0 }

# Copy under a temporary name, then rename - a copy cut short never looks complete.
$tmp = "$dest.part"
[System.IO.File]::Copy($src, $tmp, $true)
[System.IO.File]::Move($tmp, $dest)
exit 0
