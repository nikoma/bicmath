#!/usr/bin/env sh
# Build the WASM package and generate web/node bindings.
#
# Requires: rustup target add wasm32-unknown-unknown
#           cargo install wasm-bindgen-cli --version <matching wasm-bindgen>
set -eu
cd "$(dirname "$0")/.."

WASM=target/wasm32-unknown-unknown/release/bicmath_wasm.wasm

cargo build -p bicmath-wasm --target wasm32-unknown-unknown --release
wasm-bindgen --target web --out-dir examples/wasm/pkg-web --out-name bicmath_wasm "$WASM"
wasm-bindgen --target nodejs --out-dir examples/wasm/pkg-node --out-name bicmath_wasm "$WASM"

echo "wrote examples/wasm/pkg-web and examples/wasm/pkg-node"
echo "run: node examples/wasm/smoke.cjs"
echo "then serve examples/wasm over HTTP and open index.html"
