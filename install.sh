#!/bin/sh
# =============================================================================
# CAVS Node installer — installs the latest published release via curl:
#
#   curl -fsSL https://raw.githubusercontent.com/orelvis15/cavs-hub-cli/main/install.sh | sh
#
# Downloads two binaries from GitHub Releases and puts them on your PATH:
#   - cav              the CAVS Node CLI          (orelvis15/cavs-hub-cli)
#   - cavs-lfs-agent   the Git LFS transfer agent (orelvis15/cavs-oss, best-effort)
#
# Environment overrides:
#   CAVS_VERSION       version to install, e.g. 0.1.0 (default: latest)
#   CAVS_INSTALL_DIR   where to install (default: /usr/local/bin, else ~/.local/bin)
#   CAV_CLI_REPO       CLI repo (default: orelvis15/cavs-hub-cli)
#   CAV_AGENT_REPO     agent repo (default: orelvis15/cavs-oss)
# =============================================================================
set -eu

CLI_REPO="${CAV_CLI_REPO:-orelvis15/cavs-hub-cli}"
AGENT_REPO="${CAV_AGENT_REPO:-orelvis15/cavs-oss}"
VERSION="${CAVS_VERSION:-latest}"

say()  { printf '%s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
err()  { printf 'error: %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

# --- detect platform --------------------------------------------------------
detect_target() {
  os="$(uname -s)"; arch="$(uname -m)"
  case "$os" in
    Linux)  os_part="unknown-linux-gnu" ;;
    Darwin) os_part="apple-darwin" ;;
    *) err "unsupported OS: $os (build from source: see docs/install_source.md)" ;;
  esac
  case "$arch" in
    x86_64|amd64)  arch_part="x86_64" ;;
    arm64|aarch64) arch_part="aarch64" ;;
    *) err "unsupported architecture: $arch" ;;
  esac
  printf '%s-%s' "$arch_part" "$os_part"
}

# --- build a GitHub release download URL ------------------------------------
# usage: asset_url <owner/repo> <asset-name>
asset_url() {
  if [ "$VERSION" = "latest" ]; then
    printf 'https://github.com/%s/releases/latest/download/%s' "$1" "$2"
  else
    tag="$VERSION"; case "$tag" in v*) ;; *) tag="v$tag" ;; esac
    printf 'https://github.com/%s/releases/download/%s/%s' "$1" "$tag" "$2"
  fi
}

choose_dir() {
  if [ -n "${CAVS_INSTALL_DIR:-}" ]; then printf '%s' "$CAVS_INSTALL_DIR"; return; fi
  if [ -w /usr/local/bin ] 2>/dev/null; then printf '/usr/local/bin'; return; fi
  printf '%s/.local/bin' "$HOME"
}

download() {  # url dest  -> returns non-zero on failure
  if have curl; then curl -fsSL "$1" -o "$2"
  elif have wget; then wget -q "$1" -O "$2"
  else err "need curl or wget to download"; fi
}

main() {
  target="$(detect_target)"
  dir="$(choose_dir)"
  mkdir -p "$dir"
  tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT

  say "Installing CAVS Node tools ($VERSION, $target) into $dir"

  # cav — required.
  say "  downloading cav"
  download "$(asset_url "$CLI_REPO" "cav-$target")" "$tmp/cav" \
    || err "failed to download cav for $target ($VERSION)"
  chmod +x "$tmp/cav"; mv "$tmp/cav" "$dir/cav"

  # cavs-lfs-agent — best-effort (needed only for the LFS commands).
  say "  downloading cavs-lfs-agent"
  if download "$(asset_url "$AGENT_REPO" "cavs-lfs-agent-$target")" "$tmp/cavs-lfs-agent"; then
    chmod +x "$tmp/cavs-lfs-agent"; mv "$tmp/cavs-lfs-agent" "$dir/cavs-lfs-agent"
  else
    warn "could not download cavs-lfs-agent (LFS commands will be unavailable until it is installed)"
  fi

  say ""
  say "Installed into $dir: cav$( [ -x "$dir/cavs-lfs-agent" ] && printf ', cavs-lfs-agent' )"
  case ":$PATH:" in
    *":$dir:"*) : ;;
    *) say "NOTE: $dir is not on your PATH. Add it, e.g.:"
       say "      echo 'export PATH=\"$dir:\$PATH\"' >> ~/.profile" ;;
  esac
  say ""
  say "Next steps:"
  say "  cav login"
  say "  cav repo connect <org>/<repo>"
  say "  git push"
}

main "$@"
