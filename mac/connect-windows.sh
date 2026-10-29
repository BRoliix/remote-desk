#!/usr/bin/env bash
# Control the Windows PC from this Mac. Default: full desktop in Windows App. --ssh: terminal.
set -euo pipefail
source "$(dirname "$0")/common.sh"
load_config

if [ "${1:-}" = "--ssh" ]; then
  exec ssh win
fi

if ! port_open "$WIN_HOST" 3389; then
  bad "Windows '$WIN_HOST' not reachable on 3389" "./mac/doctor.sh"
  exit 1
fi

rdp="$REPO_ROOT/profiles/windows.rdp"
sed -e "s|{{WIN_HOST}}|$WIN_HOST|" -e "s|{{WIN_USER}}|$WIN_USER|" \
  "$REPO_ROOT/profiles/windows.rdp.template" > "$rdp"
open -a "Windows App" "$rdp"
