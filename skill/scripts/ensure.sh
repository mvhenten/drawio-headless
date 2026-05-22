#!/bin/sh
# Verify `drawio-headless` is on PATH. Prints install instructions if it
# isn't. Intended to be called by the SKILL on first use, or manually by
# the user.
#
# Flags
# -----
#   --quiet  Suppress all output; exit non-zero if the binary is missing.
#            Useful for shell composition: `ensure.sh --quiet || install`.
#
# Exit codes
# ----------
#   0   `drawio-headless` is on PATH
#   1   missing, or `--quiet` and missing

set -eu

QUIET=0
for arg in "$@"; do
    case "$arg" in
        --quiet) QUIET=1 ;;
        -h|--help)
            cat <<'EOF'
Usage: ensure.sh [--quiet]

Checks for the `drawio-headless` binary on PATH. If absent, prints
copy-pasteable install instructions and exits 1. With --quiet, prints
nothing and exits 1 silently when the binary is missing.
EOF
            exit 0
            ;;
        *)
            echo "ensure.sh: unknown argument: $arg" >&2
            exit 2
            ;;
    esac
done

if command -v drawio-headless >/dev/null 2>&1; then
    if [ "$QUIET" -eq 0 ]; then
        version=$(drawio-headless --version 2>/dev/null || echo "drawio-headless (unknown version)")
        echo "ok: $version"
    fi
    exit 0
fi

if [ "$QUIET" -eq 0 ]; then
    cat >&2 <<'EOF'
drawio-headless is not on PATH.

Install with Cargo (requires Rust 1.85+):

    cargo install --git https://github.com/mvhenten/drawio-headless \
                  --path crates/cli

Or clone and build locally:

    git clone https://github.com/mvhenten/drawio-headless
    cd drawio-headless
    cargo install --path crates/cli

After installation the binary will be at `~/.cargo/bin/drawio-headless`.
Ensure that directory is on PATH (`export PATH="$HOME/.cargo/bin:$PATH"`).

This skill does not install the binary automatically by design.
EOF
fi
exit 1
