#!/bin/sh
# Round-trip smoke test: invoke `drawio-headless compose` the way the skill
# would, against a small AWS spec. Asserts the output is a non-empty SVG.
#
# Usage
# -----
#   bash skill/test/round-trip.sh
#
# Exits 0 on success, non-zero on any failure (binary missing, compose
# failed, output empty or malformed).
#
# This test is intentionally not wired into `cargo test` — it exercises
# the *installed* binary, not the in-tree build. Run it after a
# `cargo install --path crates/cli` to validate the install end-to-end.

set -eu

if ! command -v drawio-headless >/dev/null 2>&1; then
    echo "round-trip.sh: drawio-headless is not on PATH" >&2
    echo "round-trip.sh: run skill/scripts/ensure.sh for install instructions" >&2
    exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

spec="$tmp/api-lambda.json"
out="$tmp/api-lambda.svg"

cat > "$spec" <<'JSON'
{
  "name": "ApiLambda",
  "nodes": [
    {"id": "api", "kind": "aws.api_gateway", "label": "API",    "x": 80,  "y": 80},
    {"id": "lam", "kind": "aws.lambda",      "label": "Lambda", "x": 320, "y": 80}
  ],
  "edges": [
    {"source": "api", "target": "lam"}
  ]
}
JSON

drawio-headless compose "$spec" "$out"

if [ ! -s "$out" ]; then
    echo "round-trip.sh: $out is missing or empty" >&2
    exit 1
fi

first_chars=$(head -c 4 "$out")
if [ "$first_chars" != "<svg" ]; then
    echo "round-trip.sh: $out does not start with <svg (got: $first_chars)" >&2
    exit 1
fi

bytes=$(wc -c < "$out" | tr -d ' ')
echo "round-trip.sh: ok ($bytes bytes at $out)"
