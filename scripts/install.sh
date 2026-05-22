#!/bin/sh
# One-line installer for `drawio-headless`. Downloads the latest pre-built
# binary from GitHub Releases for the current OS/arch into $INSTALL_DIR
# (default ~/.local/bin).
#
# Usage
# -----
#   curl -fsSL https://raw.githubusercontent.com/mvhenten/drawio-headless/main/scripts/install.sh | sh
#
# Env overrides
# -------------
#   INSTALL_DIR   target directory for the binary (default: $HOME/.local/bin)
#   VERSION       specific version to install, e.g. `v0.1.0` (default: latest)

set -eu

REPO="mvhenten/drawio-headless"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${VERSION:-latest}"
BIN_NAME="drawio-headless"

log() { printf '[install] %s\n' "$*"; }
err() { printf '[install] error: %s\n' "$*" >&2; }

# ---- detect OS / arch ------------------------------------------------------

uname_s="$(uname -s)"
uname_m="$(uname -m)"

case "$uname_s" in
    Linux)   os="unknown-linux-gnu" ;;
    Darwin)  os="apple-darwin" ;;
    MINGW*|MSYS*|CYGWIN*)
        err "Windows is not supported by this script. Use the npm package or download the .zip from GitHub Releases:"
        err "  https://github.com/${REPO}/releases"
        exit 1
        ;;
    *)
        err "Unsupported OS: $uname_s"
        exit 1
        ;;
esac

case "$uname_m" in
    x86_64|amd64)   arch="x86_64" ;;
    aarch64|arm64)  arch="aarch64" ;;
    *)
        err "Unsupported architecture: $uname_m"
        exit 1
        ;;
esac

target="${arch}-${os}"

# macOS uses `arm64` as the rustc triple component; align if needed.
case "$target" in
    aarch64-apple-darwin) target="aarch64-apple-darwin" ;;
esac

# ---- pick download URL -----------------------------------------------------

if [ "$VERSION" = "latest" ]; then
    # The /releases/latest/download/<asset> redirect works for any asset
    # name pattern. We still need to discover the version to construct the
    # asset name. Use the GitHub API redirect.
    log "resolving latest release of $REPO"
    tag="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/${REPO}/releases/latest" \
        | sed 's|.*/tag/||')"
    if [ -z "$tag" ]; then
        err "could not determine latest release tag"
        exit 1
    fi
    VERSION="$tag"
fi

version_no_v="${VERSION#v}"
asset="${BIN_NAME}-${version_no_v}-${target}.tar.gz"
url="https://github.com/${REPO}/releases/download/${VERSION}/${asset}"

log "downloading $url"

# ---- download + extract ----------------------------------------------------

tmp="$(mktemp -d -t drawio-headless-install.XXXXXX)"
trap 'rm -rf "$tmp"' EXIT INT TERM

if command -v curl >/dev/null 2>&1; then
    curl -fSL --proto '=https' --tlsv1.2 -o "$tmp/$asset" "$url"
elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$tmp/$asset" "$url"
else
    err "neither curl nor wget available"
    exit 1
fi

if [ ! -s "$tmp/$asset" ]; then
    err "download produced an empty file ($tmp/$asset)"
    exit 1
fi

tar -xzf "$tmp/$asset" -C "$tmp"

src="$(find "$tmp" -name "$BIN_NAME" -type f -perm -u+x 2>/dev/null | head -n 1)"
if [ -z "$src" ]; then
    # `-perm -u+x` is not portable on macOS; fall back without it.
    src="$(find "$tmp" -name "$BIN_NAME" -type f | head -n 1)"
fi
if [ -z "$src" ]; then
    err "binary $BIN_NAME not found in extracted archive"
    exit 1
fi

# ---- install --------------------------------------------------------------

mkdir -p "$INSTALL_DIR"
dest="$INSTALL_DIR/$BIN_NAME"
mv "$src" "$dest"
chmod +x "$dest"

log "installed $dest"

# ---- verify ---------------------------------------------------------------

if "$dest" --version >/dev/null 2>&1; then
    version_out="$("$dest" --version 2>/dev/null || echo unknown)"
    log "verified: $version_out"
else
    err "warning: installed binary at $dest did not run successfully"
fi

# ---- PATH hint ------------------------------------------------------------

case ":$PATH:" in
    *":$INSTALL_DIR:"*)
        log "$INSTALL_DIR is already on PATH"
        ;;
    *)
        log "NOTE: $INSTALL_DIR is not on your PATH."
        log "Add this to your shell rc:"
        log "  export PATH=\"$INSTALL_DIR:\$PATH\""
        ;;
esac
