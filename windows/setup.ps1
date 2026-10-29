#Requires -RunAsAdministrator
<#
.SYNOPSIS
  Makes this Windows PC a remote-desk host (RDP + SSH over Tailscale) and a client for the Mac (VNC + SSH).
  Safe to re-run: every step checks before it changes anything.

.PARAMETER MacPublicKey
  The Mac's SSH public key (printed by mac/setup.sh). Added to administrators_authorized_keys.

.PARAMETER SkipInstall
  Don't run winget (useful offline).
#>
param(
    [string]$MacPublicKey,
    [switch]$SkipInstall
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

$logDir = Join-Path $RepoRoot 'logs'
New-Item -ItemType Directory -Force $logDir | Out-Null
Start-Transcript -Path (Join-Path $logDir "setup-$(Get-Date -Format yyyyMMdd-HHmmss).log") | Out-Null

try {
    # --- 1. Software -------------------------------------------------------
    if (-not $SkipInstall) {
        Write-Step 'Installing Tailscale and TigerVNC viewer (winget)'
        foreach ($id in 'Tailscale.Tailscale', 'TigerVNC.TigerVNC') {
            $installed = winget list --id $id --exact --accept-source-agreements 2>$null | Select-String $id
            if ($installed) { Write-Ok "$id already installed"; continue }
            winget install --id $id --exact --silent --accept-package-agreements --accept-source-agreements
            if ($LASTEXITCODE -ne 0) { Write-Warn "winget returned $LASTEXITCODE for $id" } else { Write-Ok "$id installed" }
        }
    }

    # --- 2. RDP host -------------------------------------------------------
    Write-Step 'Remote Desktop host'
    $ts = 'HKLM:\System\CurrentControlSet\Control\Terminal Server'
    Set-ItemProperty $ts -Name fDenyTSConnections -Value 0
    Set-ItemProperty "$ts\WinStations\RDP-Tcp" -Name UserAuthentication -Value 1
    Set-Service TermService -StartupType Automatic
    if ((Get-Service TermService).Status -ne 'Running') { Start-Service TermService }
    Write-Ok 'RDP enabled, NLA on, TermService automatic'

    # --- 3. Firewall: only tailnet + local subnet ---------------------------
    Write-Step 'Firewall rules (scoped to Tailscale + local subnet)'
    $remote = @($TailnetCidr, 'LocalSubnet')
    $rules = @(
        @{ Name = 'remote-desk-rdp-tcp'; Display = 'remote-desk RDP (TCP 3389)'; Protocol = 'TCP'; Port = 3389 },
        @{ Name = 'remote-desk-rdp-udp'; Display = 'remote-desk RDP (UDP 3389)'; Protocol = 'UDP'; Port = 3389 },
        @{ Name = 'remote-desk-ssh';     Display = 'remote-desk SSH (TCP 22)';   Protocol = 'TCP'; Port = 22 },
        @{ Name = 'remote-desk-engine-signal'; Display = 'remote-desk engine viewer (TCP 47080)'; Protocol = 'TCP'; Port = 47080 },
        @{ Name = 'remote-desk-engine-media';  Display = 'remote-desk engine media (UDP 47081)';  Protocol = 'UDP'; Port = 47081 }
    )
    foreach ($r in $rules) {
        Get-NetFirewallRule -Name $r.Name -ErrorAction SilentlyContinue | Remove-NetFirewallRule
        New-NetFirewallRule -Name $r.Name -DisplayName $r.Display -Group $RuleGroup -Direction Inbound `
            -Action Allow -Protocol $r.Protocol -LocalPort $r.Port -RemoteAddress $remote -Profile Any | Out-Null
        Write-Ok $r.Display
    }
    # Built-in rules allow 3389 from anywhere; ours replace them.
    Get-NetFirewallRule -DisplayGroup 'Remote Desktop' -ErrorAction SilentlyContinue |
        Where-Object Enabled -eq 'True' | Disable-NetFirewallRule
    Get-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -ErrorAction SilentlyContinue | Disable-NetFirewallRule
    Write-Ok 'Built-in any-address RDP/SSH rules disabled'

    # --- 4. OpenSSH server --------------------------------------------------
    Write-Step 'OpenSSH Server'
    if (-not (Get-Service sshd -ErrorAction SilentlyContinue)) {
        # winget downloads directly; the Windows capability goes through Windows Update and can hang for ages.
        winget install --id Microsoft.OpenSSH.Preview --exact --silent --accept-package-agreements --accept-source-agreements
        if (-not (Get-Service sshd -ErrorAction SilentlyContinue)) {
            Write-Warn 'winget OpenSSH install failed, falling back to Windows capability (slow)'
            $cap = Get-WindowsCapability -Online -Name 'OpenSSH.Server*' | Select-Object -First 1
            Add-WindowsCapability -Online -Name $cap.Name | Out-Null
        }
    }
    # Both installers add their own any-address port 22 rule; ours (above) replaces it.
    Get-NetFirewallRule -ErrorAction SilentlyContinue |
        Where-Object { $_.DisplayName -like '*OpenSSH*' -and $_.Group -ne $RuleGroup -and $_.Enabled -eq 'True' } |
        Disable-NetFirewallRule
    Set-Service sshd -StartupType Automatic
    if ((Get-Service sshd).Status -ne 'Running') { Start-Service sshd }
    New-Item -Path 'HKLM:\SOFTWARE\OpenSSH' -Force | Out-Null
    Set-ItemProperty 'HKLM:\SOFTWARE\OpenSSH' -Name DefaultShell `
        -Value "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe"
    Write-Ok 'sshd running (default shell: PowerShell)'

    if ($MacPublicKey) {
        # Admin accounts read keys from here, not ~/.ssh/authorized_keys.
        $authKeys = Join-Path $env:ProgramData 'ssh\administrators_authorized_keys'
        $existing = @()
        if (Test-Path $authKeys) { $existing = Get-Content $authKeys }
        if ($existing -notcontains $MacPublicKey.Trim()) {
            Add-Content -Path $authKeys -Value $MacPublicKey.Trim() -Encoding ascii
        }
        icacls $authKeys /inheritance:r /grant 'Administrators:F' /grant 'SYSTEM:F' | Out-Null
        Write-Ok 'Mac public key authorized'
    }

    # This PC's own key, for SSH into the Mac.
    $userSsh = Join-Path $env:USERPROFILE '.ssh'
    $keyPath = Join-Path $userSsh 'id_ed25519'
    New-Item -ItemType Directory -Force $userSsh | Out-Null
    if (-not (Test-Path $keyPath)) {
        & ssh-keygen -t ed25519 -N '""' -f $keyPath -C "$env:USERNAME@$env:COMPUTERNAME" | Out-Null
    }
    Write-Ok "SSH key: $keyPath.pub"

    # --- 5. Power: stay reachable --------------------------------------------
    Write-Step 'Power settings (plugged in)'
    powercfg /setacvalueindex SCHEME_CURRENT SUB_BUTTONS LIDACTION 0   # lid close: do nothing
    powercfg /setacvalueindex SCHEME_CURRENT SUB_SLEEP STANDBYIDLE 0    # never sleep
    powercfg /setacvalueindex SCHEME_CURRENT SUB_SLEEP HIBERNATEIDLE 0  # never hibernate
    powercfg /setactive SCHEME_CURRENT
    Write-Ok 'Lid close = do nothing, sleep/hibernate = never (on AC)'
    foreach ($nic in Get-NetAdapter -Physical | Where-Object Status -eq 'Up') {
        try {
            Disable-NetAdapterPowerManagement -Name $nic.Name -ErrorAction Stop
            Write-Ok "Power saving off: $($nic.Name)"
        } catch {
            Write-Warn "Could not change power management on $($nic.Name)"
        }
    }

    # --- 6. Config + summary ---------------------------------------------------
    Write-Step 'Config'
    $email = Get-MicrosoftAccountEmail
    $status = Get-TailscaleStatus
    $winHost = $env:COMPUTERNAME.ToLower()
    if ($status -and $status.Self.DNSName) { $winHost = $status.Self.DNSName.Split('.')[0] }
    Set-ConfigValue WIN_HOST $winHost
    Set-ConfigValue WIN_SSH_USER $env:USERNAME
    if ($email) { Set-ConfigValue WIN_USER $email }
    Write-Ok "Wrote $ConfigPath"
    Install-SshConfig (Read-Config)
    Write-Ok "ssh aliases 'win' / 'mac' in ~/.ssh/config (set MAC_HOST/MAC_USER in config.env, then re-run)"

    $tsIp = $null
    if ($status -and $status.Self.TailscaleIPs) { $tsIp = $status.Self.TailscaleIPs[0] }

    Write-Host ''
    Write-Host '------------------------------------------------------------' -ForegroundColor Cyan
    Write-Host ' Connect FROM THE MAC with Windows App:'
    Write-Host "   PC name  : $winHost   (Tailscale IP: $(if ($tsIp) { $tsIp } else { 'run tailscale up first' }))"
    Write-Host "   Username : MicrosoftAccount\$email"
    Write-Host '   Password : your Microsoft account password (NOT the PIN)'
    Write-Host ''
    Write-Host ' IMPORTANT: lock this PC (Win+L) and sign in ONCE with the password,'
    Write-Host ' not the PIN, so Windows caches it for Remote Desktop.'
    Write-Host ''
    if (-not $tsIp) {
        Write-Host ' Tailscale is not logged in yet. Run:  tailscale up   (or use the tray icon)' -ForegroundColor Yellow
    }
    Write-Host ' Give this to mac/setup.sh so the Mac trusts this PC for SSH:'
    Write-Host "   ./mac/setup.sh --add-key `"$((Get-Content "$keyPath.pub").Trim())`""
    Write-Host '------------------------------------------------------------' -ForegroundColor Cyan
}
finally {
    Stop-Transcript | Out-Null
}
