# discussion-to-ai for Windows - does what deliver.sh does (BUG-311).
#
# ASCII on purpose: Windows PowerShell 5.1 reads a script without a BOM in the
# system code page. The payload arrives on stdin as UTF-8.
$ErrorActionPreference = 'Stop'
# BUG-332: UTF-8 both ways. Without this a failure message thrown below reaches openguild in
# the code page (CP949 on Korean Windows) and shows up garbled, and the child powershell's
# output (made UTF-8 below) would be read back wrong. openguild starts us with no console
# window, so this console is our own - the user's terminal is not touched.
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)

$reader = New-Object System.IO.StreamReader([Console]::OpenStandardInput(), (New-Object System.Text.UTF8Encoding($false)))
$raw = $reader.ReadToEnd()
try { $e = ConvertFrom-Json $raw } catch { $e = $null }

if ($e) {
    $target = if ($e.target) { $e.target } else { '?' }
    $author = if ($e.author) { $e.author } else { '?' }
    $msg = "[openguild] $target - $author`n$($e.question)"
    # BUG-332: to reply you must know WHERE. deliver.sh passed these; this one did not.
    $env:OG_TARGET_KIND = [string]$e.target_kind
    $env:OG_TARGET_ID = [string]$e.target_id
    $env:OG_COMMENT_ID = [string]$e.comment_id
} else {
    $msg = "[openguild] $raw"
}

# BUG-329: leave one line saying where it went - a command or a tmux pane gives
# no visible result. It lands in the data folder, which the consent screen names.
# DEV-425: the hook runs in the guild folder - a relative path would land inside the
# guild and show up in git. Write to the data folder by absolute path.
function Receipt($what, $where) {
    $stamp = Get-Date -Format 'yyyy-MM-dd HH:mm:ss'
    Add-Content -LiteralPath (Join-Path $env:OPENGUILD_PLUGIN_DATA_DIR 'delivered.log') -Value "$stamp  $what  $where" -Encoding utf8
}

$how = $env:HOW
if (-not $how) { $how = 'inbox' }

switch ($how) {
    'command' {
        if (-not $env:AI_COMMAND) { throw "discussion-to-ai: '실행할 명령' 이 비어 있습니다." }
        # BUG-332: two traps here, both fatal on Windows PowerShell 5.1.
        #
        # 1. A native command writing to stderr becomes a NativeCommandError, and with
        #    $ErrorActionPreference = 'Stop' that THROWS - a normal progress line from
        #    `claude -p` killed the script. Success is the exit code, not stderr.
        # 2. The pipe into a native command uses $OutputEncoding, ASCII by default, so a
        #    Korean question arrived as '?'. Make it UTF-8 first.
        $prevPref = $ErrorActionPreference
        $prevEnc = $OutputEncoding
        $ErrorActionPreference = 'Continue'
        $OutputEncoding = New-Object System.Text.UTF8Encoding($false)
        try {
            # BUG-333: **가리지 않고 모은다.** 전에는 stderr 의 ErrorRecord 만 골라 담았는데,
            # 로그인 안내 같은 정작 중요한 말은 stdout 으로 나온다 — 그래서 사용자에게는
            # `exit code: 1` 만 보였다(admin). 무엇이 나왔든 실패했을 때 보여 줄 수 있어야 한다.
            # BUG-335: the command runs in POWERSHELL, not cmd.exe. cmd was never the fix for
            # BUG-332 - stderr, encoding and the missing OG_* vars were - and it forced people to
            # write %VAR% in a PowerShell script, against every example around it. A child
            # powershell takes the message on stdin the same way cmd did, and `exit $LASTEXITCODE`
            # carries the inner command's code back out (-Command alone reports 0/1).
            #
            # BUG-332 (again, found on a real Windows 11):
            # - The child has its own encodings. `claude -p | openguild ...` inside it read the
            #   answer in the code page and passed it on in ASCII, so the reply was all '?'.
            #   Setting them here, in the parent, does not reach the child - set them in it.
            # - A command that does not exist starts nothing, so $LASTEXITCODE stays empty and
            #   `exit $LASTEXITCODE` said 0: the log read success while no reply came. Look at
            #   $? too, right after the command.
            # - `-Command <string>` hands the text to powershell.exe as a native argument and
            #   PowerShell 5.1 drops the inner double quotes: `Write-Output "a b"` ran as two
            #   words and `cmd /c "echo hi & exit 3"` did not parse. -EncodedCommand carries the
            #   text untouched (UTF-16LE, base64). No progress records - with it they come back
            #   on stderr as CLIXML.
            $prelude = @(
                '$ProgressPreference = ''SilentlyContinue'''
                '[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)'
                '$OutputEncoding = New-Object System.Text.UTF8Encoding($false)'
            ) -join "`n"
            $epilogue = @(
                '$ok = $?; $code = $LASTEXITCODE'
                'if ($code) { exit $code } elseif (-not $ok) { exit 1 } else { exit 0 }'
            ) -join "`n"
            $inner = $prelude + "`n" + $env:AI_COMMAND + "`n" + $epilogue
            $encoded = [Convert]::ToBase64String([System.Text.Encoding]::Unicode.GetBytes($inner))
            $out = $msg | & powershell -NoProfile -NonInteractive -EncodedCommand $encoded 2>&1
            $code = $LASTEXITCODE
        } finally {
            $ErrorActionPreference = $prevPref
            $OutputEncoding = $prevEnc
        }
        if ($code -ne 0) {
            # Say WHY, not just that it failed - the command's own words are the answer.
            # Keep the tail: the useful line is usually the last one, and a whole build
            # log in delivered.log helps nobody.
            $why = if ($out) {
                (@($out | ForEach-Object { $_.ToString().Trim() } |
                    Where-Object { $_ }) | Select-Object -Last 3) -join ' / '
            } else { '' }
            Receipt '명령 실패' "$env:AI_COMMAND -> $code $why"
            throw "명령이 $code 로 끝났습니다: $env:AI_COMMAND $why"
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
