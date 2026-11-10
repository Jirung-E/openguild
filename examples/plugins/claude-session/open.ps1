# Opens the claude session named after the quest (Windows). Input on stdin: {"name":"DEV-001"}
#
# Keep this file ASCII-only: Windows PowerShell 5.1 reads a BOM-less script in the local code page.
#
# - one session with that name -> resume it (claude -r <session id>)
# - several -> claude -r <name>, which shows a picker filtered by that name
# - none -> create it (claude -n <name>). If FIRST_PROMPT is set (the 'first message' setting, {ID} -> the name),
#   it goes along as the first message, so the session knows which document it is for.
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

# Count sessions with this name the way `claude -r <name>` picks them (claude 2.1): read only the first and
# last 64KB of each file (fast even with many large session files); the name is the last customTitle in the
# tail, else in the head, else the auto title (aiTitle). Case and surrounding spaces are ignored.
function Get-Ends([string]$path) {
    $n = 65536
    $fs = [IO.File]::Open($path, 'Open', 'Read', 'ReadWrite')
    try {
        $buf = New-Object byte[] $n
        $read = $fs.Read($buf, 0, $n)
        $head = [Text.Encoding]::UTF8.GetString($buf, 0, $read)
        $tail = $head
        if ($fs.Length -gt $n) {
            [void]$fs.Seek($fs.Length - $n, 'Begin')
            $read = $fs.Read($buf, 0, $n)
            $tail = [Text.Encoding]::UTF8.GetString($buf, 0, $read)
        }
        return @($head, $tail)
    } finally { $fs.Dispose() }
}
function Get-Last([string]$text, [string]$key) {
    $ms = [regex]::Matches($text, '"' + $key + '": ?"([^"]*)"')
    if ($ms.Count -gt 0) { return $ms[$ms.Count - 1].Groups[1].Value }
    return $null
}
$want = $name.ToLowerInvariant()
$ids = @()
if (Test-Path -LiteralPath $store) {
    Get-ChildItem -LiteralPath $store -Filter *.jsonl | ForEach-Object {
        try { $head, $tail = Get-Ends $_.FullName } catch { return }
        $t = Get-Last $tail 'customTitle'
        if (-not $t) { $t = Get-Last $head 'customTitle' }
        if (-not $t) { $t = Get-Last $tail 'aiTitle' }
        if (-not $t) { $t = Get-Last $head 'aiTitle' }
        if ($t -and $t.Trim().ToLowerInvariant() -eq $want) { $ids += $_.BaseName }
    }
}
if ($ids.Count -eq 0) { $claude = @('-n', $name) }
elseif ($ids.Count -eq 1) { $claude = @('-r', $ids[0]) }
else { $claude = @('-r', $name) }

# The first message is never spliced into the command text (BUG-387): it goes to a temp file, and the new shell
# reads that file into the argument. (The file stays in %TEMP% - deleting it would need ';', which wt.exe splits on.)
if ($ids.Count -eq 0 -and $env:FIRST_PROMPT) {
    $pf = [IO.Path]::GetTempFileName()
    [IO.File]::WriteAllText($pf, $env:FIRST_PROMPT.Replace('{ID}', $name), (New-Object Text.UTF8Encoding $false))
    $claude += "(Get-Content -Raw -Encoding UTF8 -LiteralPath '" + $pf.Replace("'", "''") + "')"
}

if ($env:CLAUDE_SESSION_DRY_RUN -eq '1') {
    Write-Output ("cd `"$dir`" ; claude " + ($claude -join ' '))
    exit 0
}

if (Get-Command wt.exe -ErrorAction SilentlyContinue) {
    # wt starts the command with CreateProcess, which can't find an npm-installed claude (claude.cmd) - go through powershell.
    Start-Process wt.exe -ArgumentList (@('-d', "`"$dir`"", 'powershell', '-NoExit', '-Command', 'claude') + $claude)
} else {
    Start-Process powershell -WorkingDirectory $dir -ArgumentList (@('-NoExit', '-Command', 'claude') + $claude)
}
