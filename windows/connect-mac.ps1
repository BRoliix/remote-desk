<#
.SYNOPSIS
  Control the Mac from this PC. Default: screen (TigerVNC). -Ssh: terminal.
#>
param([switch]$Ssh)

. (Join-Path $PSScriptRoot 'common.ps1')
$cfg = Read-Config
$macHost = $cfg.MAC_HOST

if ($Ssh) {
    & ssh "$($cfg.MAC_USER)@$macHost"
    exit $LASTEXITCODE
}

$vnc = Join-Path $env:ProgramFiles 'TigerVNC\vncviewer.exe'
if (-not (Test-Path $vnc)) { Write-Bad 'TigerVNC viewer not installed' 'winget install TigerVNC.TigerVNC'; exit 1 }
if (-not (Test-TcpPort $macHost 5900)) {
    Write-Bad "Mac '$macHost' not reachable on 5900" 'run windows\doctor.ps1'
    exit 1
}
# Log in with the Mac username + password when prompted.
Start-Process $vnc -ArgumentList "${macHost}::5900"
