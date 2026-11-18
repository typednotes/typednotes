#!/usr/bin/env bash
# Run the browser and branding release checks locally or in CI.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

cargo check --locked -p web --features web --target wasm32-unknown-unknown
cargo clippy --locked -p web --features web --target wasm32-unknown-unknown --all-targets -- -D warnings
"${PYTHON:-python3}" scripts/test_logo.py
