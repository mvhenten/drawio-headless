#!/usr/bin/env bash
# Regenerate the demo-page artefacts under docs/examples/.
set -euo pipefail

cd "$(dirname "$0")/.."

for ex in petshop cross_account event_driven; do
    cargo run -q -p drawio-headless-examples --example "$ex"
done

ls -1 docs/examples/
