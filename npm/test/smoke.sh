#!/bin/sh
# End-to-end smoke test for the npm wrapper.
#
# Packs the package in this directory, installs the resulting tarball
# globally into a temp PREFIX, runs `drawio-headless --version`, then
# `drawio-headless render` on a tiny fixture. Cleans up on exit.
#
# Requires:
#   - npm + node >= 18 on PATH
#   - network access (postinstall downloads the binary from GitHub Releases
#     matching the version in npm/package.json)
#
# Pre-release testing: point `DRAWIO_HEADLESS_LOCAL_BINARY` at a locally
# built binary (e.g. `target/release/drawio-headless`). The script will set
# `DRAWIO_HEADLESS_SKIP_DOWNLOAD=1` during `npm install` and copy the binary
# into the installed package's `vendor/` directory afterwards. Useful before
# v0.1.0 is tagged.

set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
PKG_DIR="$(dirname "$HERE")"
WORK="$(mktemp -d -t drawio-headless-smoke.XXXXXX)"
PREFIX="$WORK/prefix"

cleanup() {
    rm -rf "$WORK"
}
trap cleanup EXIT INT TERM

echo "[smoke] packing $PKG_DIR"
cd "$PKG_DIR"
TARBALL="$(npm pack --silent --pack-destination "$WORK")"

echo "[smoke] installing $TARBALL into $PREFIX"
mkdir -p "$PREFIX"
if [ -n "${DRAWIO_HEADLESS_LOCAL_BINARY:-}" ]; then
    DRAWIO_HEADLESS_SKIP_DOWNLOAD=1 npm install -g --prefix "$PREFIX" "$WORK/$TARBALL" 1>&2
    VENDOR_DIR="$PREFIX/lib/node_modules/drawio-headless/vendor"
    mkdir -p "$VENDOR_DIR"
    cp "$DRAWIO_HEADLESS_LOCAL_BINARY" "$VENDOR_DIR/drawio-headless"
    chmod +x "$VENDOR_DIR/drawio-headless"
    echo "[smoke] (local mode) staged $DRAWIO_HEADLESS_LOCAL_BINARY into $VENDOR_DIR"
else
    npm install -g --prefix "$PREFIX" "$WORK/$TARBALL" 1>&2
fi

BIN="$PREFIX/bin/drawio-headless"
if [ ! -x "$BIN" ]; then
    echo "[smoke] FAIL: $BIN not found or not executable" >&2
    exit 1
fi

echo "[smoke] drawio-headless --version"
"$BIN" --version

FIXTURE="$WORK/fixture.drawio"
cat > "$FIXTURE" <<'EOF'
<mxfile><diagram><mxGraphModel><root>
<mxCell id="0"/><mxCell id="1" parent="0"/>
<mxCell id="2" vertex="1" parent="1" value="A" style="rounded=0">
  <mxGeometry x="40" y="40" width="80" height="40" as="geometry"/>
</mxCell>
</root></mxGraphModel></diagram></mxfile>
EOF

OUT="$WORK/out.svg"
echo "[smoke] drawio-headless render $FIXTURE $OUT"
"$BIN" render "$FIXTURE" "$OUT"
if [ ! -s "$OUT" ]; then
    echo "[smoke] FAIL: $OUT is empty" >&2
    exit 1
fi
if ! head -c 200 "$OUT" | grep -q "<svg"; then
    echo "[smoke] FAIL: $OUT does not start with <svg" >&2
    exit 1
fi

echo "[smoke] OK"
