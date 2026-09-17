# desktop-notify for Windows - does what notify.sh does (BUG-294).
#
# ASCII on purpose: Windows PowerShell 5.1 reads a script without a BOM in the
# system code page. The event JSON arrives on stdin as UTF-8.
$ErrorActionPreference = 'Stop'

$reader = New-Object System.IO.StreamReader([Console]::OpenStandardInput(), (New-Object System.Text.UTF8Encoding($false)))
$e = ConvertFrom-Json $reader.ReadToEnd()

$title = 'openguild'
if ($e.event) { $title = "openguild - $($e.event)" }
$body = ''
if ($e.quest) { $body = "$($e.quest.id) $($e.quest.title)" }
elseif ($e.comment) { $body = [string]$e.comment.body }
$body = $body.Trim()
if ($body -eq '') { $body = $title }   # a balloon with empty text throws
if ($body.Length -gt 200) { $body = $body.Substring(0, 200) }

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$icon = New-Object System.Windows.Forms.NotifyIcon
$icon.Icon = [System.Drawing.SystemIcons]::Information
$icon.Visible = $true
# Windows 10/11 shows this as a toast. Keep the icon alive while it is up,
# then remove it - otherwise a dead icon stays in the tray.
$icon.ShowBalloonTip(4000, $title, $body, [System.Windows.Forms.ToolTipIcon]::Info)
Start-Sleep -Seconds 4
$icon.Dispose()
