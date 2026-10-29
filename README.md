# remote-desk

Full two-way remote control between my Windows laptop and my Mac, from any network.

| Direction | How | Command |
|---|---|---|
| Mac → Windows | Windows App (RDP) | `./mac/connect-windows.sh` |
| Windows → Mac | TigerVNC → macOS Screen Sharing | `.\windows\connect-mac.ps1` |
| Terminal / files | SSH both ways | `ssh win` · `ssh mac` · `scp`, `rsync` |

Everything runs over **Tailscale**, a private network between the two machines. They reach each other by name (`it`, `<mac>`) from home, the office or a hotspot, with no router port forwarding. The firewall only accepts RDP and SSH from the Tailscale range and the local subnet.

## First-time setup

### 1. Windows (PowerShell **as Administrator**)
```powershell
git clone https://github.com/BRoliix/remote-desk.git $HOME\Documents\remote-desk
cd $HOME\Documents\remote-desk
powershell -ExecutionPolicy Bypass -File .\windows\setup.ps1
tailscale up            # log in (or use the tray icon)
```
Then lock the PC (Win+L) and sign in **once with your Microsoft password, not the PIN**.

### 2. Mac (Terminal)
```bash
git clone https://github.com/BRoliix/remote-desk.git ~/remote-desk
cd ~/remote-desk
./mac/setup.sh          # installs Tailscale + Windows App, guides Screen Sharing on
```
Log in to Tailscale with the **same account**.

### 3. Exchange SSH keys
Each setup script prints a ready-to-paste command for the other machine:
- Mac's key → Windows: `.\windows\setup.ps1 -MacPublicKey "ssh-ed25519 ..."`
- Windows' key → Mac: `./mac/setup.sh --add-key "ssh-ed25519 ..."`

Set `MAC_HOST` and `MAC_USER` in Windows' `config.env`, and re-run `setup.ps1` to refresh the ssh aliases.

## Day to day
```bash
./mac/doctor.sh                 # Mac: is everything OK?
./mac/connect-windows.sh        # open the Windows desktop
./mac/connect-windows.sh --ssh  # Windows terminal
```
```powershell
.\windows\doctor.ps1            # Windows: is everything OK?
.\windows\connect-mac.ps1       # open the Mac desktop
.\windows\connect-mac.ps1 -Ssh  # Mac terminal
```

## Config
`config.env` (git-ignored, one per machine; template in `config.example.env`):

| Key | Meaning |
|---|---|
| `WIN_HOST` | Windows Tailscale name |
| `WIN_USER` | Microsoft account email (RDP login) |
| `WIN_SSH_USER` | Windows profile name for SSH |
| `MAC_HOST` | Mac Tailscale name |
| `MAC_USER` | Mac short username |

## Layout
```
windows/  setup.ps1 · doctor.ps1 · connect-mac.ps1 · common.ps1
mac/      setup.sh  · doctor.sh  · connect-windows.sh · common.sh
profiles/ windows.rdp.template · ssh_config.snippet
docs/     TROUBLESHOOTING.md
```

When something breaks, see **[docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md)**.
