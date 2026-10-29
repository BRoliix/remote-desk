# Shared helpers, sourced by the other mac/*.sh scripts.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG_PATH="$REPO_ROOT/config.env"

load_config() {
  local f
  for f in "$REPO_ROOT/config.example.env" "$CONFIG_PATH"; do
    [ -f "$f" ] || continue
    while IFS='=' read -r key value; do
      [[ "$key" =~ ^[A-Z_]+$ ]] || continue
      value="${value%%$'\r'}"
      export "$key=$value"
    done < "$f"
  done
}

set_config() {
  local key="$1" value="$2"
  [ -f "$CONFIG_PATH" ] || cp "$REPO_ROOT/config.example.env" "$CONFIG_PATH"
  if grep -q "^$key=" "$CONFIG_PATH"; then
    sed -i '' "s|^$key=.*|$key=$value|" "$CONFIG_PATH"
  else
    echo "$key=$value" >> "$CONFIG_PATH"
  fi
}

tailscale_bin() {
  if command -v tailscale >/dev/null 2>&1; then
    command -v tailscale
  elif [ -x /Applications/Tailscale.app/Contents/MacOS/Tailscale ]; then
    echo /Applications/Tailscale.app/Contents/MacOS/Tailscale
  fi
}

port_open() { nc -z -G 3 "$1" "$2" >/dev/null 2>&1; }

install_ssh_config() {
  # Replaces the marked remote-desk block in ~/.ssh/config, leaving everything else alone.
  mkdir -p ~/.ssh && chmod 700 ~/.ssh
  touch ~/.ssh/config
  local block
  block="$(sed -e "s|{{WIN_HOST}}|$WIN_HOST|" -e "s|{{WIN_SSH_USER}}|$WIN_SSH_USER|" \
               -e "s|{{MAC_HOST}}|$MAC_HOST|" -e "s|{{MAC_USER}}|$MAC_USER|" \
               "$REPO_ROOT/profiles/ssh_config.snippet")"
  sed -i '' '/# >>> remote-desk >>>/,/# <<< remote-desk <<</d' ~/.ssh/config
  printf '\n%s\n' "$block" >> ~/.ssh/config
  chmod 600 ~/.ssh/config
}

step() { printf '\n\033[36m==> %s\033[0m\n' "$1"; }
ok()   { printf '  \033[32m[PASS]\033[0m %s\n' "$1"; }
bad()  { printf '  \033[31m[FAIL]\033[0m %s\n' "$1"; [ -n "${2:-}" ] && printf '         \033[33mfix: %s\033[0m\n' "$2"; return 0; }
warn() { printf '  \033[33m[WARN]\033[0m %s\n' "$1"; }
