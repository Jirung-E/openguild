# discussion-to-ai for Windows - does what deliver.sh does (BUG-311).
#
# ASCII on purpose: Windows PowerShell 5.1 reads a script without a BOM in the
# system code page. The payload arrives on stdin as UTF-8.
$ErrorActionPreference = 'Stop'

$reader = New-Object System.IO.StreamReader([Console]::OpenStandardInput(), (New-Object System.Text.UTF8Encoding($false)))
$raw = $reader.ReadToEnd()
try { $e = ConvertFrom-Json $raw } catch { $e = $null }

if ($e) {
    $target = if ($e.target) { $e.target } else { '?' }
    $author = if ($e.author) { $e.author } else { '?' }
    $msg = "[openguild] $target - $author`n$($e.question)"
} else {
    $msg = "[openguild] $raw"
}

# BUG-329: leave one line saying where it went - a command or a tmux pane gives
# no visible result. It lands in the work folder, which the consent screen names.
function Receipt($what, $where) {
    $stamp = Get-Date -Format 'yyyy-MM-dd HH:mm:ss'
    Add-Content -LiteralPath 'delivered.log' -Value "$stamp  $what  $where" -Encoding utf8
}

$how = $env:HOW
if (-not $how) { $how = 'inbox' }

switch ($how) {
    'command' {
        if (-not $env:AI_COMMAND) { throw "discussion-to-ai: '넘길 프로그램' 이 비어 있습니다." }
        # A shell one-liner, same as on the other platforms - the text goes to its stdin.
        $msg | & cmd.exe /c $env:AI_COMMAND
        if ($LASTEXITCODE -ne 0) {
            Receipt '명령 실패' $env:AI_COMMAND
            throw "명령이 $LASTEXITCODE 로 끝났습니다: $env:AI_COMMAND"
        }
        Receipt '명령' $env:AI_COMMAND
    }
    'tmux' {
        # tmux is not a Windows program, but it is there under WSL / MSYS setups.
        if (-not $env:TMUX_TARGET) { throw "discussion-to-ai: 'tmux 창' 이 비어 있습니다." }
        $msg | & tmux load-buffer -
        & tmux paste-buffer -d -t $env:TMUX_TARGET
        & tmux send-keys -t $env:TMUX_TARGET Enter
        Receipt 'tmux' $env:TMUX_TARGET
    }
    default {
        $out = $env:INBOX_PATH
        if (-not $out) {
            $dir = $env:OPENGUILD_PLUGIN_DATA_DIR
            if (-not $dir) { $dir = '.' }
            $out = Join-Path $dir 'inbox.md'
        }
        $stamp = Get-Date -Format 'yyyy-MM-dd HH:mm:ss'
        Add-Content -LiteralPath $out -Value "## $stamp" -Encoding utf8
        Add-Content -LiteralPath $out -Value '' -Encoding utf8
        Add-Content -LiteralPath $out -Value $msg -Encoding utf8
        Add-Content -LiteralPath $out -Value '' -Encoding utf8
        Receipt '파일' $out
    }
}
