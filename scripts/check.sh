#!/usr/bin/env sh
# Full verification: formatting, lints, tests, feature matrix, docs, WASM smoke.
set -eu
cd "$(dirname "$0")/.."

echo "==> cargo fmt --check"
cargo fmt --all -- --check

echo "==> cargo clippy (workspace, all targets)"
cargo clippy --workspace --all-targets -- -D warnings

echo "==> cargo test (workspace)"
cargo test --workspace

echo "==> doc tests"
cargo test --workspace --doc

echo "==> MCP HTTP interop (http feature)"
cargo test -p bicmath-cli --features http

echo "==> feature matrix"
cargo build -p bicmath-engine --no-default-features --features arithmetic
cargo build -p bicmath-engine --no-default-features --features arithmetic,scientific,units
cargo build -p bicmath-engine --no-default-features --features arithmetic,statistics,finance
cargo build -p bicmath-engine --no-default-features
cargo build --workspace

echo "==> generated docs are current"
cargo run -q -p bicmath-cli -- docs --out docs/methods >/dev/null 2>&1
if command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  git diff --exit-code -- docs/methods
fi

echo "==> license report is current"
./scripts/license-report.sh
if command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  git diff --exit-code -- THIRD_PARTY_LICENSES.md
fi

echo "==> wasm smoke (requires wasm32 target and wasm-bindgen CLI)"
if rustup target list --installed | grep -q wasm32-unknown-unknown && command -v wasm-bindgen >/dev/null 2>&1; then
  ./scripts/build-wasm.sh
  node examples/wasm/smoke.cjs
else
  echo "skipped: wasm32 target or wasm-bindgen CLI not installed"
fi

echo "all checks passed"
