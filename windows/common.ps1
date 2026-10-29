# Shared helpers, dot-sourced by the other windows/*.ps1 scripts.

$RepoRoot   = Split-Path -Parent $PSScriptRoot
$ConfigPath = Join-Path $RepoRoot 'config.env'
$TailnetCidr = '100.64.0.0/10'
$RuleGroup   = 'remote-desk'

function Read-Config {
    $cfg = @{}
    foreach ($path in @((Join-Path $RepoRoot 'config.example.env'), $ConfigPath)) {
        if (-not (Test-Path $path)) { continue }
        foreach ($line in Get-Content $path) {
            if ($line -match '^\s*([A-Z_]+)\s*=\s*(.*?)\s*$') { $cfg[$Matches[1]] = $Matches[2] }
        }
    }
    return $cfg
}

function Set-ConfigValue([string]$Key, [string]$Value) {
    if (-not (Test-Path $ConfigPath)) {
        Copy-Item (Join-Path $RepoRoot 'config.example.env') $ConfigPath
    }
    $lines = Get-Content $ConfigPath
    if ($lines -match "^$Key=") {
        $lines = $lines -replace "^$Key=.*", "$Key=$Value"
    } else {
        $lines += "$Key=$Value"
    }
    [IO.File]::WriteAllLines($ConfigPath, [string[]]$lines)
}

function Get-TailscaleExe {
    $cmd = Get-Command tailscale -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    $path = Join-Path $env:ProgramFiles 'Tailscale\tailscale.exe'
    if (Test-Path $path) { return $path }
    return $null
}

function Get-TailscaleStatus {
    $ts = Get-TailscaleExe
    if (-not $ts) { return $null }
    try { return (& $ts status --json 2>$null | Out-String | ConvertFrom-Json) } catch { return $null }
}

function Test-TcpPort([string]$HostName, [int]$Port, [int]$TimeoutMs = 2500) {
    $client = New-Object System.Net.Sockets.TcpClient
    try {
        $task = $client.ConnectAsync($HostName, $Port)
        return ($task.Wait($TimeoutMs) -and $client.Connected)
    } catch {
        return $false
    } finally {
        $client.Dispose()
    }
}

function Get-MicrosoftAccountEmail {
    $key = 'HKCU:\Software\Microsoft\IdentityCRL\UserExtendedProperties'
    $item = Get-ChildItem $key -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($item) { return $item.PSChildName }
    return $null
}

function Install-SshConfig($cfg) {
    # Replaces the marked remote-desk block in ~/.ssh/config, leaving everything else alone.
    $sshDir = Join-Path $env:USERPROFILE '.ssh'
    $target = Join-Path $sshDir 'config'
    New-Item -ItemType Directory -Force $sshDir | Out-Null
    $block = Get-Content (Join-Path $RepoRoot 'profiles\ssh_config.snippet') -Raw
    foreach ($k in 'WIN_HOST', 'WIN_SSH_USER', 'MAC_HOST', 'MAC_USER') { $block = $block.Replace("{{$k}}", $cfg[$k]) }
    $current = ''
    if (Test-Path $target) { $current = Get-Content $target -Raw }
    $current = [regex]::Replace($current, '(?s)# >>> remote-desk >>>.*?# <<< remote-desk <<<\r?\n?', '')
    [IO.File]::WriteAllText($target, ($current.TrimEnd() + "`n`n" + $block).TrimStart())
}

function Test-IsAdmin {
    $id = [Security.Principal.WindowsIdentity]::GetCurrent()
    return (New-Object Security.Principal.WindowsPrincipal $id).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Write-Step([string]$Text) { Write-Host "`n==> $Text" -ForegroundColor Cyan }
function Write-Ok([string]$Text)   { Write-Host "  [PASS] $Text" -ForegroundColor Green }
function Write-Bad([string]$Text, [string]$Fix) {
    Write-Host "  [FAIL] $Text" -ForegroundColor Red
    if ($Fix) { Write-Host "         fix: $Fix" -ForegroundColor Yellow }
}
function Write-Warn([string]$Text) { Write-Host "  [WARN] $Text" -ForegroundColor Yellow }
