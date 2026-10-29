#!/usr/bin/env bash
# Read-only health check for remote-desk on the Mac. Prints PASS/FAIL with the fix for each.
set -uo pipefail
source "$(dirname "$0")/common.sh"
load_config
fails=0
fail() { fails=$((fails + 1)); bad "$@"; }

step "Tailscale"
TS="$(tailscale_bin || true)"
if [ -z "$TS" ]; then
  fail "Tailscale not installed" "brew install --cask tailscale-app"
else
  state="$("$TS" status --json 2>/dev/null | /usr/bin/python3 -c 'import json,sys; print(json.load(sys.stdin)["BackendState"])' 2>/dev/null || echo unknown)"
  if [ "$state" = "Running" ]; then ok "Tailscale connected"; else fail "Tailscale not connected ($state)" "open the Tailscale menu bar app and log in"; fi
fi

step "Reach Windows ($WIN_HOST)"
port_open "$WIN_HOST" 3389 && ok "RDP (3389) reachable" || fail "RDP (3389) unreachable" "Windows: run windows\\doctor.ps1; check Tailscale on both; check Local Network permission for Windows App"
port_open "$WIN_HOST" 22 && ok "SSH (22) reachable" || fail "SSH (22) unreachable" "Windows: run windows\\setup.ps1 as admin"

step "This Mac as host"
port_open localhost 5900 && ok "Screen Sharing on" || fail "Screen Sharing off" "System Settings > General > Sharing > Screen Sharing"
port_open localhost 22 && ok "Remote Login (SSH) on" || fail "Remote Login off" "System Settings > General > Sharing > Remote Login"

step "Client"
[ -d "/Applications/Windows App.app" ] && ok "Windows App installed" || fail "Windows App missing" "brew install --cask windows-app"
[ -f ~/.ssh/id_ed25519 ] && ok "SSH key present" || fail "No SSH key" "./mac/setup.sh"
grep -q '# >>> remote-desk >>>' ~/.ssh/config 2>/dev/null && ok "ssh aliases installed" || fail "ssh aliases missing" "./mac/setup.sh"

step "Power"
sleep_ac="$(pmset -g custom | awk '/AC Power/{f=1} f && $1=="sleep"{print $2; exit}')"
[ "$sleep_ac" = "0" ] && ok "No sleep on charger" || fail "Mac sleeps on charger (sleep=$sleep_ac)" "sudo pmset -c sleep 0"
pmset -g batt | grep -q "AC Power" || warn "On battery. Keep it plugged in for remote use."

echo
if [ "$fails" -eq 0 ]; then printf '\033[32mAll checks passed.\033[0m\n'; else printf '\033[31m%d check(s) failed.\033[0m\n' "$fails"; fi
exit "$fails"
