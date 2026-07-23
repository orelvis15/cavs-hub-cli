#!/usr/bin/env bash
# =============================================================================
# Build & install the CAVS Node CLI from source (local development install).
#
#   ./build-install.sh
#
# Installs:
#   - cav              the CAVS Node CLI       (this crate, cavs-hub-cli)
#   - cavs-lfs-agent   the Git LFS agent      (from the sibling cavs-oss repo,
#                                              if present)
#
# The LFS agent lives in the separate `cavs-oss` repository. By default this
# script looks for it next to this repo (../cavs-oss); override with
# $CAVS_OSS_DIR, or skip it entirely with --no-agent.
#
# By default it uses `cargo install` into ~/.cargo/bin (already on a Rust
# developer's PATH). To install elsewhere:
#   CAVS_INSTALL_DIR=/usr/local/bin  ./build-install.sh
#   ./build-install.sh --dir ~/bin
# =============================================================================
set -euo pipefail

# --- resolve paths ----------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CAVS_OSS_DIR="${CAVS_OSS_DIR:-$(cd "$SCRIPT_DIR/.." && pwd)/cavs-oss}"

INSTALL_DIR="${CAVS_INSTALL_DIR:-}"
WITH_AGENT=1
while [ $# -gt 0 ]; do
  case "$1" in
    --dir) INSTALL_DIR="$2"; shift 2 ;;
    --no-agent) WITH_AGENT=0; shift ;;
    -h|--help) sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 1 ;;
  esac
done

say()  { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33mwarning:\033[0m %s\n' "$*" >&2; }
err()  { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

command -v cargo >/dev/null 2>&1 || err "cargo not found — install Rust from https://rustup.rs"

AGENT_CRATE="$CAVS_OSS_DIR/core/cavs-lfs-agent"
if [ "$WITH_AGENT" -eq 1 ] && [ ! -d "$AGENT_CRATE" ]; then
  warn "cavs-lfs-agent crate not found at $AGENT_CRATE"
  warn "skipping the LFS agent (set \$CAVS_OSS_DIR to the cavs-oss checkout, or pass --no-agent)"
  WITH_AGENT=0
fi

# --- install ----------------------------------------------------------------
if [ -n "$INSTALL_DIR" ]; then
  # Explicit directory: build release binaries and copy them in.
  mkdir -p "$INSTALL_DIR"

  say "Building cav (Hub CLI) [release]"
  ( cd "$SCRIPT_DIR" && cargo build --release )
  install -m 0755 "$SCRIPT_DIR/target/release/cav" "$INSTALL_DIR/cav"

  if [ "$WITH_AGENT" -eq 1 ]; then
    say "Building cavs-lfs-agent [release]"
    ( cd "$AGENT_CRATE" && cargo build --release )
    install -m 0755 "$CAVS_OSS_DIR/target/release/cavs-lfs-agent" "$INSTALL_DIR/cavs-lfs-agent"
  fi
  BIN_DIR="$INSTALL_DIR"
else
  # Default: cargo install into ~/.cargo/bin (--force to overwrite prior builds).
  say "Installing cav (Hub CLI) via cargo install"
  cargo install --path "$SCRIPT_DIR" --force

  if [ "$WITH_AGENT" -eq 1 ]; then
    say "Installing cavs-lfs-agent via cargo install"
    cargo install --path "$AGENT_CRATE" --force
  fi
  BIN_DIR="${CARGO_HOME:-$HOME/.cargo}/bin"
fi

# --- verify -----------------------------------------------------------------
say "Installed into $BIN_DIR:"
"$BIN_DIR/cav" --version
[ "$WITH_AGENT" -eq 1 ] && "$BIN_DIR/cavs-lfs-agent" --version || true

case ":$PATH:" in
  *":$BIN_DIR:"*) : ;;
  *) printf '\n\033[1;33mNOTE:\033[0m %s is not on your PATH. Add it, e.g.:\n' "$BIN_DIR"
     printf "      echo 'export PATH=\"%s:\$PATH\"' >> ~/.zshrc\n" "$BIN_DIR" ;;
esac

cat <<EOF

Done. Try it:
  cav status
  export CAVS_API=http://localhost:8080     # point at your local backend
  cav login                                 # paste a cavs_ token from the dashboard
  cav repo connect <org>/<repo>
EOF
