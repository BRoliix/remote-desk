<#
.SYNOPSIS
  Read-only health check for remote-desk on Windows. Prints PASS/FAIL with the fix for each.
#>
. (Join-Path $PSScriptRoot 'common.ps1')
$cfg = Read-Config
$fails = 0
function Fail([string]$t, [string]$f) { $script:fails++; Write-Bad $t $f }

Write-Step 'Remote Desktop host'
$ts = Get-ItemProperty 'HKLM:\System\CurrentControlSet\Control\Terminal Server'
if ($ts.fDenyTSConnections -eq 0) { Write-Ok 'RDP enabled' } else { Fail 'RDP disabled' 'run windows\setup.ps1 as admin' }
$nla = (Get-ItemProperty 'HKLM:\System\CurrentControlSet\Control\Terminal Server\WinStations\RDP-Tcp').UserAuthentication
if ($nla -eq 1) { Write-Ok 'NLA on' } else { Write-Warn 'NLA off (works, but less secure)' }
$svc = Get-Service TermService
if ($svc.Status -eq 'Running') { Write-Ok 'TermService running' } else { Fail 'TermService not running' 'Start-Service TermService' }
if (Get-NetTCPConnection -LocalPort 3389 -State Listen -ErrorAction SilentlyContinue) { Write-Ok 'Listening on 3389' } else { Fail 'Nothing listening on 3389' 'run windows\setup.ps1 as admin' }
$pwless = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\PasswordLess\Device' -ErrorAction SilentlyContinue).DevicePasswordLessBuildVersion
if ($pwless -eq 2) { Fail 'Windows Hello-only sign-in is on (blocks RDP passwords)' 'Settings > Accounts > Sign-in options > turn off "only allow Windows Hello sign-in"' } else { Write-Ok 'Password sign-in allowed' }

Write-Step 'SSH server'
$sshd = Get-Service sshd -ErrorAction SilentlyContinue
if ($sshd -and $sshd.Status -eq 'Running') { Write-Ok 'sshd running' } else { Fail 'sshd not running/installed' 'run windows\setup.ps1 as admin' }

Write-Step 'Firewall'
foreach ($name in 'remote-desk-rdp-tcp', 'remote-desk-rdp-udp', 'remote-desk-ssh') {
    $rule = Get-NetFirewallRule -Name $name -ErrorAction SilentlyContinue
    if ($rule -and $rule.Enabled -eq 'True') { Write-Ok "$name enabled" } else { Fail "$name missing" 'run windows\setup.ps1 as admin' }
}
$open = Get-NetFirewallRule -DisplayGroup 'Remote Desktop' -ErrorAction SilentlyContinue | Where-Object Enabled -eq 'True'
if ($open) { Write-Warn 'Built-in Remote Desktop rules still allow any address (setup.ps1 disables them)' }

Write-Step 'Tailscale'
$status = Get-TailscaleStatus
if (-not (Get-TailscaleExe)) {
    Fail 'Tailscale not installed' 'winget install Tailscale.Tailscale'
} elseif (-not $status -or $status.BackendState -ne 'Running') {
    Fail "Tailscale not connected (state: $($status.BackendState))" 'tailscale up'
} else {
    Write-Ok "Connected as $($status.Self.DNSName) $($status.Self.TailscaleIPs[0])"
    $peer = $status.Peer.PSObject.Properties.Value | Where-Object { $_.HostName -eq $cfg.MAC_HOST -or $_.DNSName -like "$($cfg.MAC_HOST).*" } | Select-Object -First 1
    if ($peer) {
        if ($peer.Online) { Write-Ok "Mac '$($cfg.MAC_HOST)' online in tailnet" } else { Fail "Mac '$($cfg.MAC_HOST)' offline" 'wake the Mac / open Tailscale on it' }
    } else {
        Write-Warn "Mac '$($cfg.MAC_HOST)' not found in tailnet. Set MAC_HOST in config.env (see 'tailscale status')"
    }
}

Write-Step "Reach the Mac ($($cfg.MAC_HOST))"
if (Test-TcpPort $cfg.MAC_HOST 5900) { Write-Ok 'Screen Sharing (5900) reachable' } else { Fail 'Screen Sharing (5900) unreachable' 'Mac: System Settings > General > Sharing > Screen Sharing ON' }
if (Test-TcpPort $cfg.MAC_HOST 22)   { Write-Ok 'SSH (22) reachable' }            else { Fail 'SSH (22) unreachable' 'Mac: Sharing > Remote Login ON' }

Write-Step 'Client tools'
$vnc = Join-Path $env:ProgramFiles 'TigerVNC\vncviewer.exe'
if (Test-Path $vnc) { Write-Ok 'TigerVNC viewer installed' } else { Fail 'TigerVNC viewer missing' 'winget install TigerVNC.TigerVNC' }
if (Test-Path (Join-Path $env:USERPROFILE '.ssh\id_ed25519')) { Write-Ok 'SSH key present' } else { Fail 'No SSH key' 'run windows\setup.ps1' }

Write-Step 'Power'
$lid = powercfg /query SCHEME_CURRENT SUB_BUTTONS LIDACTION | Select-String 'Current AC Power Setting Index: (0x[0-9a-f]+)'
if ($lid -and [Convert]::ToInt32($lid.Matches[0].Groups[1].Value, 16) -eq 0) { Write-Ok 'Lid close on AC: do nothing' } else { Fail 'Lid close on AC will sleep the PC' 'run windows\setup.ps1 as admin' }
$sleep = powercfg /query SCHEME_CURRENT SUB_SLEEP STANDBYIDLE | Select-String 'Current AC Power Setting Index: (0x[0-9a-f]+)'
if ($sleep -and [Convert]::ToInt32($sleep.Matches[0].Groups[1].Value, 16) -eq 0) { Write-Ok 'Sleep on AC: never' } else { Fail 'PC sleeps on AC' 'run windows\setup.ps1 as admin' }
$battery = Get-CimInstance Win32_Battery -ErrorAction SilentlyContinue
if ($battery -and $battery.BatteryStatus -eq 1) { Write-Warn 'Running on battery. Keep it plugged in for remote use.' }

Write-Host ''
if ($fails -eq 0) { Write-Host 'All checks passed.' -ForegroundColor Green } else { Write-Host "$fails check(s) failed." -ForegroundColor Red }
exit $fails
