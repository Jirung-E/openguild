# Opens the claude session named after the quest (Windows). Input on stdin: {"name":"DEV-001"}
#
# Keep this file ASCII-only: Windows PowerShell 5.1 reads a BOM-less script in the local code page.
#
# - one session with that name -> resume it (claude -r <session id>)
# - several -> claude -r <name>, which shows a picker filtered by that name
# - none -> create it (claude -n <name>)
#
# Sessions are grouped per folder: ~/.claude/projects/<folder path with every non-alphanumeric char as ->.
# CLAUDE_SESSION_DRY_RUN=1 prints the command instead of opening a terminal (for tests).
$ErrorActionPreference = 'Stop'
[Console]::InputEncoding = [System.Text.Encoding]::UTF8
$in = [Console]::In.ReadToEnd()
$m = [regex]::Match($in, '"name"\s*:\s*"([A-Za-z0-9_-]+)"')
if (-not $m.Success) { [Console]::Error.WriteLine('claude-session: no session name'); exit 1 }
$name = $m.Groups[1].Value

$dir = $env:PROJECT_DIR
if (-not $dir) { $dir = $env:OPENGUILD_GUILD_DIR }
if (-not $dir) { $dir = '.' }
$dir = (Resolve-Path -LiteralPath $dir).Path

$root = $env:CLAUDE_CONFIG_DIR
if (-not $root) { $root = Join-Path $HOME '.claude' }
$store = Join-Path (Join-Path $root 'projects') ($dir -replace '[^A-Za-z0-9]', '-')

# Sessions whose LAST title is this name (a session can be renamed later).
$ids = @()
if (Test-Path -LiteralPath $store) {
    Get-ChildItem -LiteralPath $store -Filter *.jsonl | ForEach-Object {
        $last = Select-String -LiteralPath $_.FullName -Pattern '"type":"custom-title"' -SimpleMatch | Select-Object -Last 1
        if ($last -and $last.Line.Contains('"customTitle":"' + $name + '"')) { $ids += $_.BaseName }
    }
}
if ($ids.Count -eq 0) { $claude = @('-n', $name) }
elseif ($ids.Count -eq 1) { $claude = @('-r', $ids[0]) }
else { $claude = @('-r', $name) }

if ($env:CLAUDE_SESSION_DRY_RUN -eq '1') {
    Write-Output ("cd `"$dir`" ; claude " + ($claude -join ' '))
    exit 0
}

if (Get-Command wt.exe -ErrorAction SilentlyContinue) {
    Start-Process wt.exe -ArgumentList (@('-d', "`"$dir`"", 'claude') + $claude)
} else {
    Start-Process powershell -WorkingDirectory $dir -ArgumentList (@('-NoExit', '-Command', 'claude') + $claude)
}
