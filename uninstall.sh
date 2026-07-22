#!/bin/sh
# =============================================================================
# CAVS Hub uninstaller — removes the binaries the installer put on your PATH:
#
#   curl -fsSL https://raw.githubusercontent.com/orelvis15/cavs-hub-cli/main/uninstall.sh | sh
#
# Removes:
#   - cav              the CAVS Hub CLI
#   - cavs-lfs-agent   the Git LFS transfer agent
#
# Environment overrides:
#   CAVS_INSTALL_DIR   directory to remove from (default: auto-detect the same
#                      dirs the installer uses, plus wherever cav is on PATH)
#   CAVS_PURGE         if set to 1, also delete the config dir
#                      ($XDG_CONFIG_HOME/cav, default ~/.config/cav)
# =============================================================================
set -eu

say()  { printf '%s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
have() { command -v "$1" >/dev/null 2>&1; }

BINS="cav cavs-lfs-agent"

# Candidate directories, in priority order. A caller-provided CAVS_INSTALL_DIR
# wins; otherwise we mirror the installer's choices.
candidate_dirs() {
  if [ -n "${CAVS_INSTALL_DIR:-}" ]; then
    printf '%s\n' "$CAVS_INSTALL_DIR"
    return
  fi
  printf '%s\n' "/usr/local/bin" "$HOME/.local/bin" "$HOME/.cargo/bin"
}

remove_at() {  # path
  path="$1"
  [ -e "$path" ] || return 1
  if rm -f "$path" 2>/dev/null; then
    say "  removed $path"
    return 0
  fi
  # Permission denied — try sudo if available.
  if have sudo; then
    warn "cannot remove $path without elevated privileges; retrying with sudo"
    if sudo rm -f "$path"; then
      say "  removed $path (sudo)"
      return 0
    fi
  fi
  warn "could not remove $path (permission denied)"
  return 1
}

main() {
  say "Uninstalling CAVS Hub tools"

  for bin in $BINS; do
    # Directories we know the installer uses.
    candidate_dirs | while IFS= read -r dir; do
      [ -n "$dir" ] || continue
      remove_at "$dir/$bin" || true
    done
    # Whatever is actually resolved on PATH (covers custom locations).
    if have "$bin"; then
      resolved="$(command -v "$bin")"
      remove_at "$resolved" || true
    fi
  done

  # Optional: remove the config directory.
  cfg="${XDG_CONFIG_HOME:-$HOME/.config}/cav"
  if [ "${CAVS_PURGE:-0}" = "1" ]; then
    if [ -d "$cfg" ]; then
      rm -rf "$cfg" && say "  removed config $cfg"
    fi
  elif [ -d "$cfg" ]; then
    say ""
    say "Left config in place: $cfg"
    say "  (re-run with CAVS_PURGE=1 to delete it)"
  fi

  say ""
  if have cav; then
    warn "cav is still on your PATH at $(command -v cav) — remove it manually"
  else
    say "Done. cav and cavs-lfs-agent are no longer on your PATH."
  fi
}

main "$@"
