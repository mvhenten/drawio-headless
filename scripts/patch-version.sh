#!/usr/bin/env bash
# Patch the workspace version in Cargo.toml for a CI build, in the checkout
# only -- this is never committed. Binaries built after this runs report the
# given version (clap's `version` derive reads CARGO_PKG_VERSION, which cargo
# sets from `[workspace.package] version` at compile time).
#
# Usage:
#   scripts/patch-version.sh 0.2.0
set -euo pipefail

cd "$(dirname "$0")/.."

new="${1:?usage: patch-version.sh <version>}"

perl -0pi -e 's/(\[workspace\.package\][^\[]*?\nversion = ")[0-9]+\.[0-9]+\.[0-9]+(")/${1}'"$new"'${2}/s' Cargo.toml

grep -m1 '^version = ' Cargo.toml
