#!/bin/sh
# End-to-end test for the curl install script.
#
# Runs `scripts/install.sh` against the real GitHub Releases of the repo
# into a temp INSTALL_DIR, then exercises `--version` and `render` on a
# tiny fixture.
#
# Requires the matching release to exist on GitHub. Pre-release, you can
# stage a local binary via DRAWIO_HEADLESS_LOCAL_BINARY and skip the
# download path.
#
# Usage
# -----
#   scripts/install.sh.test.sh                              # against latest release
#   VERSION=v0.1.0 scripts/install.sh.test.sh               # specific tag
#   DRAWIO_HEADLESS_LOCAL_BINARY=$PWD/target/release/drawio-headless \
#       scripts/install.sh.test.sh                          # pre-release dry run

set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
WORK="$(mktemp -d -t drawio-headless-curl-test.XXXXXX)"
INSTALL_DIR="$WORK/bin"

cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT INT TERM

if [ -n "${DRAWIO_HEADLESS_LOCAL_BINARY:-}" ]; then
    echo "[curl-test] local mode: staging $DRAWIO_HEADLESS_LOCAL_BINARY"
    mkdir -p "$INSTALL_DIR"
    cp "$DRAWIO_HEADLESS_LOCAL_BINARY" "$INSTALL_DIR/drawio-headless"
    chmod +x "$INSTALL_DIR/drawio-headless"
else
    echo "[curl-test] running install.sh against ${VERSION:-latest} release"
    INSTALL_DIR="$INSTALL_DIR" VERSION="${VERSION:-latest}" sh "$HERE/install.sh"
fi

BIN="$INSTALL_DIR/drawio-headless"
if [ ! -x "$BIN" ]; then
    echo "[curl-test] FAIL: $BIN not present or not executable" >&2
    exit 1
fi

echo "[curl-test] $BIN --version"
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
"$BIN" render "$FIXTURE" "$OUT"
if [ ! -s "$OUT" ] || ! head -c 200 "$OUT" | grep -q '<svg'; then
    echo "[curl-test] FAIL: $OUT is empty or not SVG" >&2
    exit 1
fi

echo "[curl-test] OK"
