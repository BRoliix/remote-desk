#!/usr/bin/env bash
# Makes this Mac a remote-desk host (Screen Sharing + SSH over Tailscale) and a client for Windows (Windows App).
# Safe to re-run.
#
#   ./mac/setup.sh                       full setup
#   ./mac/setup.sh --add-key "ssh-ed25519 AAAA... green@IT"   trust the Windows PC's SSH key
set -euo pipefail
source "$(dirname "$0")/common.sh"

if [ "${1:-}" = "--add-key" ]; then
  key="${2:?usage: --add-key \"<public key>\"}"
  mkdir -p ~/.ssh && chmod 700 ~/.ssh
  touch ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys
  grep -qxF "$key" ~/.ssh/authorized_keys || echo "$key" >> ~/.ssh/authorized_keys
  ok "Windows key authorized in ~/.ssh/authorized_keys"
  exit 0
fi

# --- 1. Software -----------------------------------------------------------
step "Installing Tailscale and Windows App (Homebrew)"
if ! command -v brew >/dev/null 2>&1; then
  bad "Homebrew not installed" '/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"'
  exit 1
fi
if [ -d /Applications/Tailscale.app ]; then ok "Tailscale already installed"
else brew install --cask tailscale-app || brew install --cask tailscale; ok "Tailscale installed"; fi
if [ -d "/Applications/Windows App.app" ]; then ok "Windows App already installed"
else brew install --cask windows-app; ok "Windows App installed"; fi

# --- 2. SSH (Remote Login) -------------------------------------------------
step "Remote Login (SSH server)"
if sudo systemsetup -getremotelogin 2>/dev/null | grep -qi ': on'; then
  ok "Remote Login already on"
elif sudo systemsetup -setremotelogin on >/dev/null 2>&1; then
  ok "Remote Login turned on"
else
  warn "macOS blocked the change (Terminal needs Full Disk Access)."
  warn "Turn it on manually: System Settings > General > Sharing > Remote Login"
fi

# --- 3. Screen Sharing (GUI only on modern macOS) ----------------------------
step "Screen Sharing (VNC)"
if port_open localhost 5900; then
  ok "Screen Sharing already on"
else
  echo "  Opening Sharing settings. Turn ON 'Screen Sharing', then click (i) next to it:"
  echo "    - Allow access for: Only these users -> add your user"
  echo "    - If TigerVNC on Windows won't log in with your Mac password, enable"
  echo "      'VNC viewers may control screen with password' and set one."
  open "x-apple.systempreferences:com.apple.Sharing-Settings.extension"
  read -r -p "  Press Enter once Screen Sharing is on... "
  port_open localhost 5900 && ok "Screen Sharing on" || bad "Screen Sharing still off" "System Settings > General > Sharing"
fi

# --- 4. Local Network permission for Windows App ------------------------------
step "Local Network permission"
echo "  macOS 15+ silently blocks LAN connections for apps without this permission."
echo "  System Settings > Privacy & Security > Local Network > enable 'Windows App'."
echo "  (Opening it now. If Windows App isn't listed yet, launch it once and try a connection.)"
open "x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork" || true

# --- 5. Power ----------------------------------------------------------------
step "Power settings (on charger)"
sudo pmset -c sleep 0 displaysleep 15 womp 1 tcpkeepalive 1
ok "No system sleep on charger, wake on network on"

# --- 6. SSH key + config ---------------------------------------------------------
step "SSH key and aliases"
if [ ! -f ~/.ssh/id_ed25519 ]; then
  mkdir -p ~/.ssh && chmod 700 ~/.ssh
  ssh-keygen -t ed25519 -N "" -f ~/.ssh/id_ed25519 -C "$(whoami)@$(scutil --get LocalHostName)" >/dev/null
fi
ok "SSH key: ~/.ssh/id_ed25519.pub"

TS="$(tailscale_bin || true)"
mac_host="$(scutil --get LocalHostName | tr '[:upper:]' '[:lower:]')"
if [ -n "$TS" ]; then
  dns="$("$TS" status --json 2>/dev/null | /usr/bin/python3 -c 'import json,sys; print(json.load(sys.stdin)["Self"]["DNSName"].split(".")[0])' 2>/dev/null || true)"
  [ -n "$dns" ] && mac_host="$dns"
fi
set_config MAC_HOST "$mac_host"
set_config MAC_USER "$(whoami)"
load_config
if [ "$WIN_USER" = "you@outlook.com" ]; then
  read -r -p "  Windows PC's Microsoft account email (RDP username): " win_user
  set_config WIN_USER "$win_user"
fi
read -r -p "  Windows PC Tailscale name [$WIN_HOST]: " win_host
[ -n "$win_host" ] && set_config WIN_HOST "$win_host"
load_config
install_ssh_config
ok "config.env updated, ssh aliases 'win' / 'mac' installed"

# --- Summary -------------------------------------------------------------------
cat <<EOF

------------------------------------------------------------
 Tailscale: open the Tailscale app (menu bar) and log in with the
 SAME account as the Windows PC.

 Connect TO WINDOWS:   ./mac/connect-windows.sh
   username  MicrosoftAccount\\$WIN_USER
   password  Microsoft account password (not the PIN)

 Give this to the Windows PC (PowerShell as admin, in the repo):
   .\\windows\\setup.ps1 -MacPublicKey "$(cat ~/.ssh/id_ed25519.pub)"

 Then on Windows set MAC_HOST=$mac_host and MAC_USER=$(whoami) in config.env.
------------------------------------------------------------
EOF
