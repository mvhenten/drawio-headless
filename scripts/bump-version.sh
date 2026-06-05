#!/usr/bin/env bash
# Write a new version into every place it lives:
#   - Cargo.toml         [workspace.package] version  (all crates inherit it)
#   - Cargo.lock         the four workspace package entries
#   - npm/package.json   the npm wrapper version
#
# The Rust binary reports `version` from Cargo.toml via clap, and the npm
# postinstall reads npm/package.json to build the GitHub Release asset URL, so
# all three must agree with the tag.
#
# Usage:
#   scripts/bump-version.sh 0.2.0
set -euo pipefail

cd "$(dirname "$0")/.."

new="${1:?usage: bump-version.sh <X.Y.Z>}"
if ! printf '%s' "$new" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo "error: version must be X.Y.Z, got '$new'" >&2
    exit 1
fi

# --- Cargo.toml workspace version -------------------------------------------
# Only the line inside [workspace.package]; that block is the first `version =`
# after the `[workspace.package]` header. We rewrite the first version line in
# the file, which in this manifest is precisely that one.
perl -0pi -e 's/(\[workspace\.package\][^\[]*?\nversion = ")[0-9]+\.[0-9]+\.[0-9]+(")/${1}'"$new"'${2}/s' Cargo.toml

# --- Cargo.lock workspace package entries -----------------------------------
# Bump the version line in each of our own [[package]] entries. Match by name
# so we never touch a third-party crate that happens to share a version.
for pkg in drawio-author drawio-render drawio-headless drawio-headless-examples closed-loop-test; do
    perl -0pi -e 's/(name = "'"$pkg"'"\nversion = ")[0-9]+\.[0-9]+\.[0-9]+(")/${1}'"$new"'${2}/' Cargo.lock
done

# --- npm wrapper package.json ------------------------------------------------
node -e '
  const fs = require("fs");
  const f = "npm/package.json";
  const p = JSON.parse(fs.readFileSync(f, "utf8"));
  p.version = process.argv[1];
  fs.writeFileSync(f, JSON.stringify(p, null, 2) + "\n");
' "$new"

echo "bumped to $new:"
grep -m1 '^version = ' Cargo.toml
grep -A1 'name = "drawio-headless"' Cargo.lock | grep '^version' | head -n1
grep '"version"' npm/package.json | head -n1
