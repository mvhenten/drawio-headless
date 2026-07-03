#!/usr/bin/env bash
# Regenerate the demo-page artefacts under docs/examples/.
set -euo pipefail

cd "$(dirname "$0")/.."

for ex in petshop cross_account event_driven hybrid_identity order_search_pipeline three_tier_web streaming_lanes k8s_deployment; do
    cargo run -q -p drawio-headless-examples --example "$ex"
done

ls -1 docs/examples/
