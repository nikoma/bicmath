#!/usr/bin/env sh
# Generate THIRD_PARTY_LICENSES.md from the resolved dependency graph.
set -eu
cd "$(dirname "$0")/.."
cargo metadata --format-version 1 2>/dev/null | python3 scripts/license_report.py > THIRD_PARTY_LICENSES.md
echo "wrote THIRD_PARTY_LICENSES.md"
