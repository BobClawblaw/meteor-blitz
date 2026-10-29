#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --target wasm32-unknown-unknown --release
mkdir -p web
cp target/wasm32-unknown-unknown/release/meteor-blitz.wasm web/meteor-blitz.wasm
if [ ! -f web/mq_js_bundle.js ]; then
  curl -sL -o web/mq_js_bundle.js https://not-fl3.github.io/miniquad-samples/mq_js_bundle.js
fi
echo "web build ready: serve the repo root and open / (index.html)"
