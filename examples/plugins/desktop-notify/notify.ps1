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

# BUG-309: who the notification says it is from.
#
# Windows takes the name and icon in the toast header from the app id (AUMID)
# of whoever sends it. A tray balloon sent from powershell.exe therefore reads
# "Windows PowerShell" - which is what the user saw.
#
# The desktop app's installer registers the AUMID below on its Start Menu
# shortcut, so sending a real toast under that id makes the header read
# "openguild" with the app's icon. If the shortcut is not there (portable
# build, app never installed) the id is unknown to Windows and the toast is
# silently dropped - so we look for the shortcut first and keep the balloon as
# the fallback. A plain balloon is worse branding but it does show up.
$aumid = 'io.openguild.desktop'
$shortcuts = @(
    (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\openguild.lnk'),
    (Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\openguild.lnk')
)
$registered = $false
foreach ($p in $shortcuts) { if (Test-Path -LiteralPath $p) { $registered = $true } }

function Send-Toast {
    $null = [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime]
    $null = [Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime]
    $t = [System.Security.SecurityElement]::Escape($title)
    $b = [System.Security.SecurityElement]::Escape($body)
    $xml = New-Object Windows.Data.Xml.Dom.XmlDocument
    $xml.LoadXml("<toast><visual><binding template='ToastGeneric'><text>$t</text><text>$b</text></binding></visual></toast>")
    $toast = New-Object Windows.UI.Notifications.ToastNotification $xml
    [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier($aumid).Show($toast)
}

function Send-Balloon {
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
}

$sent = $false
if ($registered) {
    try { Send-Toast; $sent = $true } catch { $sent = $false }
}
if (-not $sent) { Send-Balloon }
